use crate::{
    common::{prepare_contract, Prepared},
    context::Context,
    product::RegisterProductCommand,
};

use uuid::Uuid;

#[tokio::test]
async fn coverage() -> anyhow::Result<()> {
    println!("👷🏽 Run happy flow test");

    let mut context = Context::new().await?;

    let manager = context.account("manager").await?;
    let alice = context.account("alice").await?;
    let fee_account = context.account("fee").await?;

    context.ft_contract.init().await?;
    context
        .jar_contract
        .init(context.ft_contract.account(), &fee_account, manager.id())
        .await?;

    context
        .ft_contract
        .storage_deposit(context.jar_contract.account())
        .await?;

    context.ft_contract.storage_deposit(&fee_account).await?;
    context.ft_contract.storage_deposit(&alice).await?;
    context.ft_contract.mint_for_user(&alice, 100_000_000).await?;
    context.ft_contract.mint_for_user(&manager, 100_000_000).await?;
    context
        .ft_contract
        .mint_for_user(&context.jar_contract.account(), 100_000_000)
        .await?;

    let coverage = &context.jar_contract.get_coverage().await?;
    let coverage: Vec<u8> = near_sdk::base64::decode(&coverage.logs[0]).unwrap();

    let id = Uuid::new_v4();

    std::fs::write(format!("../profraw/{id}.profraw"), coverage).unwrap();

    // std::fs::write("output.profraw", coverage).unwrap();

    Ok(())
}
