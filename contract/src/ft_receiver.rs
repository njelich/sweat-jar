use near_contract_standards::fungible_token::receiver::FungibleTokenReceiver;
use near_sdk::{json_types::U128, serde::{Deserialize, Serialize}, serde_json, PromiseOrValue};

use crate::*;
use crate::jar::model::JarTicket;
use crate::migration::model::CeFiJar;

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
#[serde(tag = "action", content = "data")]
#[serde(rename_all = "snake_case")]
pub enum FtMessage {
    Stake(StakeMessage),
    Migrate(Vec<CeFiJar>),
    TopUp(JarIndex),
}

#[derive(Serialize, Deserialize)]
#[serde(crate = "near_sdk::serde")]
pub struct StakeMessage {
    ticket: JarTicket,
    signature: Option<Base64VecU8>,
    receiver_id: Option<AccountId>,
}

#[near_bindgen]
impl FungibleTokenReceiver for Contract {
    fn ft_on_transfer(
        &mut self,
        sender_id: AccountId,
        amount: U128,
        msg: String,
    ) -> PromiseOrValue<U128> {
        self.assert_from_ft_contract();

        let ft_message: FtMessage = serde_json::from_str(&msg).unwrap();

        match ft_message {
            FtMessage::Stake(message) => {
                let receiver_id = message.receiver_id.unwrap_or_else(|| sender_id.clone());
                self.create_jar(
                    receiver_id,
                    message.ticket,
                    amount,
                    message.signature,
                );
            }
            FtMessage::Migrate(jars) => {
                self.migrate_jars(jars, amount);
            }
            FtMessage::TopUp(jar_index) => {
                self.top_up(jar_index, amount);
            }
        }

        PromiseOrValue::Value(0.into())
    }
}
