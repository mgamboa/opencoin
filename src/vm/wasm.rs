pub struct SmartContract {
    pub code: Vec<u8>,
    pub params: Vec<u8>,
    pub gas_limit: u64,
    pub gas_used: u64,
}

#[derive(Debug)]
pub enum ContractResult {
    Success(Vec<u8>),
    Error(String),
    OutOfGas,
}

impl SmartContract {
    pub fn new(code: Vec<u8>, gas_limit: u64) -> Self {
        SmartContract {
            code,
            params: Vec::new(),
            gas_limit,
            gas_used: 0,
        }
    }

    pub fn execute(&mut self) -> ContractResult {
        if self.code.is_empty() {
            return ContractResult::Error("Empty contract code".to_string());
        }
        if self.gas_limit == 0 {
            return ContractResult::OutOfGas;
        }
        self.gas_used = 10;
        ContractResult::Success(vec![0u8; 32])
    }

    pub fn estimate_gas(code: &[u8]) -> u64 {
        (code.len() as u64) * 10 + 100
    }
}
