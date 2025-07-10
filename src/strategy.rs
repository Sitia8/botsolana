use crate::model::MlModel;
use anyhow::Result;

pub struct Strategy {
    model: MlModel,
    threshold: f64,
}

impl Strategy {
    pub fn new(model: MlModel, threshold: f64) -> Self {
        Self { model, threshold }
    }

    pub fn generate_signal(&self, features: &[f64]) -> Option<OrderSide> {
        let prob = self.model.predict(features);
        if prob > self.threshold {
            Some(OrderSide::Buy)
        } else if prob < 1.0 - self.threshold {
            Some(OrderSide::Sell)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OrderSide {
    Buy,
    Sell,
}
