use crate::db::TuffDb;
use crate::models::Abstract;
use crate::pipeline::traits::{AbstractGenerator, ClaimVerifier, FactFetcher, InputSplitter};

pub struct IngestPipeline<S, F, V, G, D>
where
    S: InputSplitter,
    F: FactFetcher,
    V: ClaimVerifier,
    G: AbstractGenerator,
    D: TuffDb,
{
    pub splitter: S,
    pub fetcher: F,
    pub verifier: V,
    pub generator: G,
    pub db: D,
}

impl<S, F, V, G, D> IngestPipeline<S, F, V, G, D>
where
    S: InputSplitter,
    F: FactFetcher,
    V: ClaimVerifier,
    G: AbstractGenerator,
    D: TuffDb,
{
    pub async fn ingest(&self, input: &str) -> anyhow::Result<Vec<crate::db::OpLog>> {
        let parts = self.splitter.split(input);
        let mut ops = Vec::new();
        for fragment in parts {
            let facts = self.fetcher.fetch(&fragment).await?;
            let status = self.verifier.verify(&fragment, &facts).await?;
            let abstract_ = self
                .generator
                .generate(&fragment, &facts, status)
                .await?;
            let op = self.db.append_abstract(abstract_)?;
            ops.push(op);
        }
        Ok(ops)
    }

    pub fn select_all(&self) -> anyhow::Result<Vec<Abstract>> {
        self.db.select(crate::db::SelectQuery::default())
    }
}
