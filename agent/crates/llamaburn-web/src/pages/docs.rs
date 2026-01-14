use leptos::prelude::*;

#[component]
pub fn DocsPage() -> impl IntoView {
    view! {
        <div class="page docs-page">
            <h2>"Documentation"</h2>

            <div class="docs-toc">
                <h3>"Contents"</h3>
                <ul>
                    <li><a href="#getting-started">"Getting Started"</a></li>
                    <li><a href="#benchmark">"Benchmark Guide"</a></li>
                    <li><a href="#stress-test">"Stress Test Guide"</a></li>
                    <li><a href="#eval">"Eval Guide"</a></li>
                    <li><a href="#cli-reference">"CLI Reference"</a></li>
                </ul>
            </div>

            <section id="getting-started" class="docs-section">
                <h3>"Getting Started"</h3>

                <h4>"Prerequisites"</h4>
                <ul>
                    <li><strong>"Docker"</strong>" — For containerized deployment"</li>
                    <li><strong>"Ollama"</strong>" — Running on your host machine with models downloaded"</li>
                </ul>

                <h4>"Quick Start"</h4>
                <pre><code>"# Clone and build\ngit clone https://github.com/hubenschmidt/llamaburn\ncd llamaburn\n\n# Start the web UI\ndocker compose up web --build\n\n# Open in browser\nopen http://localhost:3001"</code></pre>

                <h4>"First Benchmark"</h4>
                <ol>
                    <li>"Navigate to the "<strong>"Benchmark"</strong>" tab"</li>
                    <li>"Select a model from the dropdown"</li>
                    <li>"Set iterations (default: 5) and warmup runs (default: 2)"</li>
                    <li>"Click "<strong>"Run Benchmark"</strong></li>
                    <li>"View results showing TTFT, TPS, and latency metrics"</li>
                </ol>
            </section>

            <section id="benchmark" class="docs-section">
                <h3>"Benchmark Guide"</h3>

                <h4>"What It Measures"</h4>
                <table class="docs-table">
                    <thead>
                        <tr>
                            <th>"Metric"</th>
                            <th>"Description"</th>
                        </tr>
                    </thead>
                    <tbody>
                        <tr>
                            <td><code>"TTFT"</code></td>
                            <td>"Time to First Token — latency before generation starts"</td>
                        </tr>
                        <tr>
                            <td><code>"TPS"</code></td>
                            <td>"Tokens Per Second — generation throughput"</td>
                        </tr>
                        <tr>
                            <td><code>"ITL"</code></td>
                            <td>"Inter-Token Latency — time between tokens"</td>
                        </tr>
                        <tr>
                            <td><code>"Total"</code></td>
                            <td>"End-to-end generation time"</td>
                        </tr>
                    </tbody>
                </table>

                <h4>"Configuration"</h4>
                <ul>
                    <li><strong>"Iterations"</strong>" — Number of test runs (more = more accurate averages)"</li>
                    <li><strong>"Warmup"</strong>" — Discarded initial runs to warm up the model"</li>
                    <li><strong>"Temperature"</strong>" — Set to 0.0 for deterministic results"</li>
                </ul>
            </section>

            <section id="stress-test" class="docs-section">
                <h3>"Stress Test Guide"</h3>

                <h4>"Test Modes"</h4>
                <table class="docs-table">
                    <thead>
                        <tr>
                            <th>"Mode"</th>
                            <th>"Description"</th>
                        </tr>
                    </thead>
                    <tbody>
                        <tr>
                            <td><strong>"Ramp"</strong></td>
                            <td>"Gradually increase concurrent requests until degradation"</td>
                        </tr>
                        <tr>
                            <td><strong>"Sweep"</strong></td>
                            <td>"Test each concurrency level from 1 to max"</td>
                        </tr>
                        <tr>
                            <td><strong>"Sustained"</strong></td>
                            <td>"Fixed load over time to measure stability"</td>
                        </tr>
                        <tr>
                            <td><strong>"Spike"</strong></td>
                            <td>"Sudden burst to measure impact and recovery"</td>
                        </tr>
                    </tbody>
                </table>

                <h4>"Metrics"</h4>
                <ul>
                    <li><strong>"P50/P95/P99"</strong>" — Latency percentiles"</li>
                    <li><strong>"Error Rate"</strong>" — Percentage of failed requests"</li>
                    <li><strong>"Degradation Point"</strong>" — Where latency exceeds 2x baseline"</li>
                    <li><strong>"Failure Point"</strong>" — Where errors exceed 5%"</li>
                </ul>
            </section>

            <section id="eval" class="docs-section">
                <h3>"Eval Guide"</h3>

                <h4>"LLM-as-Judge"</h4>
                <p>"Evaluates model responses using a frontier model (Claude/GPT) as judge."</p>

                <h4>"Scoring Criteria"</h4>
                <table class="docs-table">
                    <thead>
                        <tr>
                            <th>"Score"</th>
                            <th>"Meaning"</th>
                        </tr>
                    </thead>
                    <tbody>
                        <tr><td>"1"</td><td>"Completely wrong or unrelated"</td></tr>
                        <tr><td>"2"</td><td>"Partially correct but major errors"</td></tr>
                        <tr><td>"3"</td><td>"Mostly correct with minor issues"</td></tr>
                        <tr><td>"4"</td><td>"Correct with good detail"</td></tr>
                        <tr><td>"5"</td><td>"Perfect, comprehensive answer"</td></tr>
                    </tbody>
                </table>

                <h4>"Evaluation Dimensions"</h4>
                <ul>
                    <li><strong>"Accuracy"</strong>" — Factual correctness"</li>
                    <li><strong>"Completeness"</strong>" — Covers all aspects"</li>
                    <li><strong>"Coherence"</strong>" — Logical and clear"</li>
                </ul>
            </section>

            <section id="cli-reference" class="docs-section">
                <h3>"CLI Reference"</h3>

                <h4>"List Models"</h4>
                <pre><code>"llamaburn models"</code></pre>

                <h4>"Run Benchmark"</h4>
                <pre><code>"llamaburn benchmark <model> [options]\n\nOptions:\n  -i, --iterations <n>    Number of iterations (default: 5)\n  -w, --warmup <n>        Warmup runs (default: 2)\n  -p, --prompts <set>     Prompt set: default, coding, reasoning\n  -t, --temperature <f>   Temperature (default: 0.0)\n  -m, --max-tokens <n>    Max tokens to generate\n\nExamples:\n  llamaburn benchmark llama3.1:8b -i 10\n  llamaburn benchmark gpt-oss:latest -p coding"</code></pre>

                <h4>"Stress Test"</h4>
                <pre><code>"llamaburn stress --model <model> [options]\n\nOptions:\n  --mode <mode>           ramp, sweep, sustained, spike\n  --max-concurrency <n>   Maximum concurrent requests\n  --duration <time>       Test duration (e.g., 15m)\n  --arrival <pattern>     static or poisson\n\nExamples:\n  llamaburn stress --model llama3.1:8b --mode ramp\n  llamaburn stress --model gpt-oss:latest --mode sustained --duration 10m"</code></pre>

                <h4>"Evaluation"</h4>
                <pre><code>"llamaburn eval --model <model> --set <evalset> [options]\n\nOptions:\n  --judge <provider>      claude or openai\n  --no-web-search         Disable web search\n  --pairwise              Compare two models\n  --compare-to <model>    Model to compare against\n\nExamples:\n  llamaburn eval --model llama3.1:8b --set factual --judge claude\n  llamaburn eval --model llama3.1:8b --pairwise --compare-to mistral:7b"</code></pre>

                <h4>"System Status"</h4>
                <pre><code>"llamaburn status"</code></pre>
            </section>
        </div>
    }
}
