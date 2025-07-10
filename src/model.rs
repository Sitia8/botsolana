use anyhow::Result;

use ndarray::{Array1, Array2};
use linfa::prelude::*;
use linfa_logistic::LogisticRegression;
use log;
use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Serialize, Deserialize)]
pub struct MlModel {
    params: Vec<f64>,
}

impl MlModel {
    pub fn train(x: Array2<f64>, y: Vec<i32>) -> Result<Self> {
        // y must be 1-D array of class labels (0/1)
        let y = Array1::<i32>::from(y);
        let ds = Dataset::new(x, y);
        let model = LogisticRegression::default().fit(&ds)?;
        let params = model.params().to_vec();
        Ok(Self { params })
    }

    pub fn predict(&self, features: &[f64]) -> f64 {
        if self.params.is_empty() {
            return 0.5; // Safety fallback
        }
        let (bias, weights) = self.params.split_first().unwrap();
        let z: f64 = *bias + weights.iter().zip(features).map(|(w, x)| w * x).sum::<f64>();
        1.0 / (1.0 + (-z).exp())
    }

    pub fn save(&self, path: &str) -> Result<()> {
        let data = bincode::serialize(self)?;
        fs::write(path, data)?;
        Ok(())
    }

    pub fn load(path: &str) -> Result<Self> {
        match fs::read(path) {
            Ok(bytes) => Ok(bincode::deserialize(&bytes)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                log::warn!("Model file '{}' not found. Using zero weights until first training.", path);
                Ok(Self { params: vec![0.0, 0.0, 0.0] })
            }
            Err(e) => Err(e.into()),
        }
    }
}
