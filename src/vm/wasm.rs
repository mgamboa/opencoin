use std::collections::HashMap;
use wasmtime::*;

const ARGS_OFFSET: usize = 1024;
const RESULT_OFFSET: usize = 2048;
const DEPLOY_GAS_LIMIT: u64 = 200_000;
const CALL_GAS_LIMIT: u64 = 100_000;
const RESULT_MAX: usize = 1024;

pub struct ContractContext {
    pub caller: [u8; 32],
    pub block_height: u64,
    pub contract_address: [u8; 32],
    pub events: Vec<Vec<u8>>,
    pub persist: HashMap<String, Vec<u8>>,
}

pub struct WasmRuntime {
    engine: Engine,
}

impl WasmRuntime {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let mut config = Config::new();
        config.consume_fuel(true);
        let engine = Engine::new(&config)?;
        Ok(WasmRuntime { engine })
    }

    pub fn deploy(
        &self,
        code: &[u8],
        args: &[u8],
        ctx: &mut ContractContext,
    ) -> Result<u64, String> {
        let mut store = Store::new(&self.engine, ctx);
        store.set_fuel(DEPLOY_GAS_LIMIT).map_err(|e| format!("Fuel: {}", e))?;
        let module = Module::new(&self.engine, code).map_err(|e| format!("WASM: {}", e))?;
        let linker = Self::make_linker(&self.engine)?;
        let instance = linker.instantiate(&mut store, &module)
            .map_err(|e| format!("Instantiate: {}", e))?;
        Self::write_args(&mut store, &instance, args)?;
        let init = instance.get_typed_func::<(i32, i32), i32>(&mut store, "init")
            .map_err(|_| "Contract missing 'init'".to_string())?;
        init.call(&mut store, (ARGS_OFFSET as i32, args.len() as i32))
            .map_err(|e| format!("init: {}", e))?;
        let remaining = store.get_fuel().map_err(|e| format!("Fuel: {}", e))?;
        Ok(DEPLOY_GAS_LIMIT.saturating_sub(remaining))
    }

    pub fn call(
        &self,
        code: &[u8],
        args: &[u8],
        ctx: &mut ContractContext,
    ) -> Result<(u64, Vec<u8>), String> {
        let mut store = Store::new(&self.engine, ctx);
        store.set_fuel(CALL_GAS_LIMIT).map_err(|e| format!("Fuel: {}", e))?;
        let module = Module::new(&self.engine, code).map_err(|e| format!("WASM: {}", e))?;
        let linker = Self::make_linker(&self.engine)?;
        let instance = linker.instantiate(&mut store, &module)
            .map_err(|e| format!("Instantiate: {}", e))?;
        Self::write_args(&mut store, &instance, args)?;
        let call = instance.get_typed_func::<(i32, i32), i32>(&mut store, "call")
            .map_err(|_| "Contract missing 'call'".to_string())?;
        call.call(&mut store, (ARGS_OFFSET as i32, args.len() as i32))
            .map_err(|e| format!("call: {}", e))?;
        let result = Self::read_result(&mut store, &instance);
        let remaining = store.get_fuel().map_err(|e| format!("Fuel: {}", e))?;
        Ok((CALL_GAS_LIMIT.saturating_sub(remaining), result))
    }

    fn make_linker(engine: &Engine) -> Result<Linker<&mut ContractContext>, String> {
        let mut linker = Linker::new(engine);
        linker.func_wrap("env", "read_storage",
            |mut caller: Caller<'_, &mut ContractContext>, key_ptr: i32, key_len: i32, val_ptr: i32, max_len: i32| -> i32 {
                let key = read_mem(&mut caller, key_ptr, key_len);
                let key_s = String::from_utf8_lossy(&key).to_string();
                let data = caller.data().persist.get(&key_s).cloned();
                match data {
                    Some(v) => {
                        let n = v.len().min(max_len as usize);
                        let mem = caller.get_export("memory").and_then(|e| e.into_memory()).unwrap();
                        mem.write(caller.as_context_mut(), val_ptr as usize, &v[..n]).ok();
                        n as i32
                    }
                    None => 0,
                }
            }
        ).map_err(|e| format!("link: {}", e))?;

        linker.func_wrap("env", "write_storage",
            |mut caller: Caller<'_, &mut ContractContext>, key_ptr: i32, key_len: i32, val_ptr: i32, val_len: i32| {
                let key = read_mem(&mut caller, key_ptr, key_len);
                let val = read_mem(&mut caller, val_ptr, val_len);
                caller.data_mut().persist.insert(String::from_utf8_lossy(&key).to_string(), val);
            }
        ).map_err(|e| format!("link: {}", e))?;

        linker.func_wrap("env", "get_caller",
            |mut caller: Caller<'_, &mut ContractContext>, ptr: i32, max_len: i32| -> i32 {
                let data = caller.data().caller;
                let n = data.len().min(max_len as usize);
                let mem = caller.get_export("memory").and_then(|e| e.into_memory()).unwrap();
                mem.write(caller.as_context_mut(), ptr as usize, &data[..n]).ok();
                n as i32
            }
        ).map_err(|e| format!("link: {}", e))?;

        linker.func_wrap("env", "get_block_height",
            |caller: Caller<'_, &mut ContractContext>| -> i64 {
                caller.data().block_height as i64
            }
        ).map_err(|e| format!("link: {}", e))?;

        linker.func_wrap("env", "get_contract_address",
            |mut caller: Caller<'_, &mut ContractContext>, ptr: i32, max_len: i32| -> i32 {
                let data = caller.data().contract_address;
                let n = data.len().min(max_len as usize);
                let mem = caller.get_export("memory").and_then(|e| e.into_memory()).unwrap();
                mem.write(caller.as_context_mut(), ptr as usize, &data[..n]).ok();
                n as i32
            }
        ).map_err(|e| format!("link: {}", e))?;

        linker.func_wrap("env", "emit_event",
            |mut caller: Caller<'_, &mut ContractContext>, data_ptr: i32, data_len: i32| {
                let event = read_mem(&mut caller, data_ptr, data_len);
                caller.data_mut().events.push(event);
            }
        ).map_err(|e| format!("link: {}", e))?;

        linker.func_wrap("env", "debug_log",
            |mut caller: Caller<'_, &mut ContractContext>, ptr: i32, len: i32| {
                let msg = read_mem(&mut caller, ptr, len);
                log::info!("[WASM] {}", String::from_utf8_lossy(&msg));
            }
        ).map_err(|e| format!("link: {}", e))?;

        Ok(linker)
    }

    fn write_args(store: &mut Store<&mut ContractContext>, instance: &Instance, args: &[u8]) -> Result<(), String> {
        let mem = instance.get_memory(store.as_context_mut(), "memory")
            .ok_or("No memory export".to_string())?;
        mem.write(store.as_context_mut(), ARGS_OFFSET, args)
            .map_err(|e| format!("mem write: {}", e))?;
        Ok(())
    }

    fn read_result(store: &mut Store<&mut ContractContext>, instance: &Instance) -> Vec<u8> {
        let mem = instance.get_memory(store.as_context_mut(), "memory").unwrap();
        let mut len_buf = [0u8; 4];
        if mem.read(store.as_context(), RESULT_OFFSET, &mut len_buf).is_err() {
            return Vec::new();
        }
        let len = u32::from_le_bytes(len_buf) as usize;
        let len = len.min(RESULT_MAX - 4);
        let mut buf = vec![0u8; len];
        mem.read(store.as_context(), RESULT_OFFSET + 4, &mut buf).ok();
        buf
    }
}

fn read_mem(caller: &mut Caller<'_, &mut ContractContext>, ptr: i32, len: i32) -> Vec<u8> {
    let mem = caller.get_export("memory").and_then(|e| e.into_memory()).unwrap();
    let mut buf = vec![0u8; len as usize];
    mem.read(caller.as_context(), ptr as usize, &mut buf).ok();
    buf
}

pub fn contract_address(deployer: &[u8; 32], nonce: u64) -> [u8; 32] {
    let mut data = deployer.to_vec();
    data.extend_from_slice(&nonce.to_le_bytes());
    crate::crypto::hash::double_sha3_256(&data)
}
