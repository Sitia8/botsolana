use anchor_lang::prelude::*;

declare_id!("ACXrExQwHHz9Soxm7wzGSwiQaJMvkJaAaWMDxvWrF6vY");

#[program]
pub mod my_anchor_program {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        msg!("Greetings from: {:?}", ctx.program_id);
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Initialize {}
