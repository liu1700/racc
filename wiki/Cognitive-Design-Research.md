# Cognitive Design Research

[< Home](Home.md) | [< UI Design](UI-Design.md)

> This document synthesizes research from cognitive neuroscience, human factors engineering, developer productivity studies, and visual perception science into actionable design principles for multi-agent IDE tooling. It serves as the scientific foundation for Racc's UI/UX decisions.

## The Core Challenge

A developer monitoring 3–10 parallel AI coding agents faces a compound cognitive paradox with no precedent in software tooling:

- **Sustained vigilance** degrades after 15 minutes
- **Divided attention** is actually rapid sequential switching at 100–500ms cost per switch
- **Creative capacity** requires a brain network anti-correlated with monitoring
- **Automation complacency** worsens as AI reliability improves

The bottom line: the human brain can effectively supervise **3–5 agents with good UI design**, scaling to 10 only with aggressive automation support, intelligent aggregation, and a fundamentally different interaction model than single-agent tools provide.

---

## 1. Working Memory Caps Agent Tracking

### Cowan's Limit: 4±1 Chunks

George Miller's "7±2 chunks" has been revised downward. Nelson Cowan's 2001 reanalysis places pure working memory capacity at **4±1 chunks** when chunking strategies are controlled. A developer can maintain active awareness of only 3–5 agent sessions simultaneously without external cognitive support.

**Design implication:** Monitoring 6–10 agents requires the UI itself to do cognitive work — automatically triaging, grouping by status, and surfacing only what demands human attention.

### Cognitive Chunking for Sessions

Each agent session must be compressible into a **single cognitive chunk**: a compact visual summary (status color + task description + progress indicator + time elapsed) that communicates state without requiring sub-detail processing.

When 10 agents are grouped into 3 status categories ("running normally," "needs review," "blocked/errored"), the developer holds 3 categorical chunks rather than 10 individual items — well within Cowan's limit.

### Information Foraging Theory

Peter Pirolli and Stuart Card's information foraging theory reframes each agent session as an "information patch." The developer is a forager who must decide which agent to investigate next based on **information scent** — proximal visual cues that signal whether deeper investigation is worthwhile.

Strong scent cues include:
- Status color changes
- Error counts
- "Stuck" badges
- Time-since-last-progress indicators
- Micro-summaries (e.g., "Refactoring auth.py — 2 tests pass, 1 failing — 73% complete")

---

## 2. Attention Switching and Vigilance

### Every Switch Has a Cost

The brain does not perform parallel cognitive processing on complex tasks. What feels like multitasking is rapid sequential attention switching, with each switch costing **100–500ms** at the micro level and up to a **40% increase in total task time**.

Sophie Leroy's research on "attention residue" shows that when switching from one agent's output to another, cognitive capacity remains partially allocated to the previous session, **reducing performance by 40–60%** until the residue fades over 15–30 minutes.

### Vigilance Decrement

Norman Mackworth's foundational research shows sustained monitoring performance **degrades significantly within 15–20 minutes**, with a steep initial decline. Critically, the decrement is **more severe during automated monitoring than manual operation** — the exact scenario of watching AI agents work.

**Key finding (Pop et al., 2012):** Adding a secondary interactive task negated the vigilance decrement entirely. Passive monitoring destroys attention; active micro-engagement preserves it.

**Design implication:** Agents should periodically request lightweight human input — confirming approaches, rating code quality, answering clarification questions — maintaining the developer in an active supervisory role.

### Multiple Resource Theory (Wickens)

Tasks using different sensory channels (visual vs. auditory) share attentional resources more efficiently than tasks competing within the same modality. A developer visually focused on one agent's code can simultaneously receive auditory status signals from other agents without significant interference.

The SEEV model adds that visual scanning effort increases dramatically beyond **20° visual angle** (requiring head movement), so critical status information should remain within a compact visual field.

---

## 3. Flow State vs. Monitoring Paradox

### The Paradox

Flow requires deep, uninterrupted focus, but supervising AI agents requires divided attention and periodic evaluation. The Default Mode Network (DMN) — which generates creative insights — is **anti-correlated with the task-positive/dorsal attention network** used for focused monitoring. You cannot simultaneously monitor vigilantly and think creatively.

### The Cost of Interruption

- Only **10% of interrupted programming sessions** resume editing within one minute (Parnin & DeLine, CHI 2010)
- Full context recovery takes **10–15 minutes for simple tasks and 30–45 minutes for complex architecture work**
- Workers switch activities every **3 minutes 5 seconds** and working spheres every **10.5 minutes** (Gloria Mark)

### The METR Warning

The most rigorous RCT on AI coding productivity found **developers using AI tools took 19% longer** despite believing they were 20% faster — a 43-point perception gap. Jeremy Howard's "dark flow" concept captures this: AI-assisted coding can create a state that feels like productive flow but is measurably less efficient.

### Design Resolution: Mode Separation

The solution requires **explicit mode separation**:
- **Monitoring mode:** Simplified, ambient status displays for periodic evaluation
- **Deep work mode:** Immersive focus on a single agent or the developer's own code

The IDE should support **batched review cycles** rather than constant monitoring. When agents complete work, results should queue for review during dedicated evaluation windows rather than interrupting creative work in real time.

---

## 4. Burnout Biology and UI Influence

### The Stress Cascade

Chronic monitoring stress activates the HPA axis, producing sustained cortisol elevation that leads to:
- **Prefrontal cortex thinning** (impairing executive function)
- **Amygdala hypertrophy** (increasing stress reactivity)
- **Reduced PFC-amygdala connectivity** (degrading emotional regulation)

### Color and Arousal

- **Red** activates sympathetic "fight or flight" responses, measurably increasing heart rate
- **Blue** reduces autonomic arousal and promotes calm
- **Green** produces the strongest stress reduction (cortisol reduced 53% vs. 37% for urban routes)

**Design implication:** Reserve saturated red exclusively for true errors. Use blue/green tones for normal and completed states. Amber for warnings that need attention but aren't emergencies.

### Notification Fatigue

A 2025 study found that **notification frequency itself didn't predict cognitive outcomes** — alert fatigue and attention disruption quality were the strongest predictors. The intrusiveness and relevance of notifications matter more than their raw count.

### Decision Fatigue

Code review effectiveness plummets after **60–90 minutes**, with optimal review thresholds of **200–400 lines of code per session**. A multi-agent IDE that surfaces thousands of lines across 10 agents without batching and prioritization will exhaust this capacity rapidly.

### Break Integration

Albulescu et al.'s 2022 meta-analysis found microbreaks **boost vigor and reduce fatigue** without reducing productivity. Cornell research showed that even viewing nature images for 40 seconds improved post-break performance. The IDE should implement adaptive break prompts based on detected fatigue signals rather than rigid timers.

---

## 5. Supervisory Control Theory

### Sheridan's Five Functions

Developers perform five supervisory functions:
1. **Plan** — assign tasks to agents
2. **Teach** — configure prompts and context
3. **Monitor** — observe progress
4. **Intervene** — correct errors or redirect
5. **Learn** — review outcomes to improve future use

Michael Lewis's command complexity analysis: managing N agents should impose **O(1) cognitive load** — effort independent of group size — achieved through an intelligent orchestrator that aggregates status and escalates only what requires human judgment.

### Optimal Operator-to-Agent Ratio

Cummings et al. found the optimal bound for one operator supervising multiple autonomous vehicles was **2–4 vehicles**, constrained by human performance limitations. Scaling to 10 agents requires an intelligent intermediary layer.

### Automation Levels by Task Type

Based on the Parasuraman-Sheridan-Wickens (2000) framework:

| Task Type | Automation Level | Behavior |
|-----------|-----------------|----------|
| **Boilerplate/scaffolding** | Level 7–8 | Agent executes autonomously, informs afterward |
| **Bug fixes with test coverage** | Level 5–6 | Agent executes if developer approves within veto window |
| **Architectural decisions** | Level 3–4 | Agent suggests alternatives, developer selects |
| **Security-sensitive code** | Level 4–5 | Agent suggests approach, awaits explicit approval |
| **Information gathering** | Level 8–9 | Fully autonomous |

### Empirical Validation (Anthropic, Feb 2026)

Study of millions of human-agent interactions found:
- Experienced users auto-approve **>40% of sessions** (vs. 20% for new users) while simultaneously **interrupting more often**
- Agent-initiated stops exceed human-initiated stops on complex tasks by 2×

This demonstrates sophisticated trust calibration where developers approve routine outputs and concentrate scrutiny on anomalous ones.

---

## 6. Trust Calibration

### The Three Bases of Trust (Lee & See, 2004)

- **Performance** — what the automation does
- **Process** — how it operates
- **Purpose** — the designer's intent

Miscalibrated trust produces:
- **Misuse** (overtrust → rubber-stamping AI output)
- **Disuse** (undertrust → not delegating tasks the AI handles well)

### Out-of-the-Loop Problem

Under full automation, **Level 2 Situation Awareness (comprehension) degrades most severely** — operators can perceive raw data but lose understanding of what it means. Keeping developers involved in decision selection — even when implementation is automated — preserves comprehension needed for effective oversight.

### The Deskilling Risk

- A CMU/Microsoft study found knowledge workers **ceded problem-solving expertise to AI while becoming more confident**
- AI-assisted doctors performed worse when AI was removed (detection rates dropping from 28.4% to 22.4%)
- The FAA now recommends pilots do more manual flying to counteract automation-driven skill atrophy

**Design implication:** Include deliberate "manual coding" periods, structured walkthroughs of agent-generated output, and comprehension checks that maintain the developer's ability to evaluate code independently.

### Interaction Patterns (CHI 2025 — AGDebugger)

Users need to:
- Specify more detailed instructions
- Simplify agent tasks
- Alter agents' plans

**Checkpoint/rollback capabilities and conversation overview visualization** are essential. The IDE must support interactive steering, not just passive monitoring.

---

## 7. Visual Encoding Principles

### Preattentive Processing (Treisman)

Certain visual features are processed **preattentively — automatically and in parallel across the entire visual field in under 200ms**:

1. **Color hue** — most powerful preattentive attribute
2. **Motion/flicker** — use sparingly, only for critical alerts
3. **Size** — secondary channel for priority
4. **Orientation** — tertiary channel

A single panel displaying a saturated red or amber hue among muted-tone peers will "pop out" regardless of how many panels exist — enabling problem detection across 10 sessions in a single sub-second glance.

**Critical constraint:** Conjunction search fails preattentively. Each status dimension must map to **one unique preattentive channel**. Never require developers to identify status through a combination of features.

### Gestalt Grouping

- **Enclosure** (common region) — most reliable for dashboard sections; each agent in a card container
- **Proximity** — groups related elements within a card
- **Similarity** — consistent card layout ensures uniform parsing
- **Figure-ground** — focused panel has slightly higher contrast while others recede

### Layout Heuristics

- **Upper-left position** for highest-priority information (F-pattern eye scanning)
- Generous whitespace as a grouping tool
- Consistent visualization techniques across cards

### Typography

- **JetBrains Mono** — designed with increased x-height for readability at small sizes
- Minimum **13–14px** for code even in small panels
- Line-height of **1.4–1.5**
- Font size is more predictive of readability than font choice

### Dark Mode Design

- Default to **dark mode** (70% developer preference, lower perceived workload)
- Use dark gray backgrounds (**#1E1E1E–#282C34**, never pure black)
- Slightly muted text (**#D4D4D4–#E0E0E0**)
- Always provide a **light mode toggle** for users with astigmatism (~50% of population), dyslexia, or bright ambient conditions

---

## 8. Tiered Alert Architecture

Healthcare alarm fatigue data: **72–99% of all hospital alarms are false or clinically insignificant**. The cry-wolf effect causes desensitization, mistrust, and delayed response.

### Five-Tier Notification Model

Based on Mark Weiser and John Seely Brown's calm technology principles:

| Tier | Type | Implementation | Interruption Level |
|------|------|----------------|-------------------|
| **1** | Ambient | Peripheral color indicators per panel (green/blue/amber/red) | None — preattentive |
| **2** | Informational | Subtle panel border pulse or progress update | None — peripheral |
| **3** | Advisory | Non-blocking toast notification with soft tone | Low — agent completed, ready for review |
| **4** | Warning | Persistent yellow banner plus distinctive audio | Medium — error requiring guidance |
| **5** | Critical | Full-attention modal with urgent sound | High — test failures, security issues, data loss risk |

### Anti-Fatigue Design

- Signal-to-noise target: **above 50%**
- Smart deduplication: aggregate similar issues across agents into one notification
- Notification budgets: limit total alerts per time window
- User-controlled thresholds per tier

---

## 9. Key Design Principles (Summary)

Three non-obvious principles emerge from this synthesis:

### Principle 1: Batched Evaluation Over Continuous Monitoring

Agents work autonomously in the background while the developer alternates between their own deep work and periodic review checkpoints. Transform passive surveillance into active assessment.

### Principle 2: Categorical Chunks Over Individual Items

Managing 10 agents should feel like managing 3 status categories, not 10 individual sessions. The UI must do the cognitive work of grouping, prioritizing, and triaging so working memory holds categorical chunks (needs attention / running normally / completed).

### Principle 3: Actively Resist Reliability Complacency

The better the AI agents perform, the greater the complacency risk. Deliberate trust calibration mechanisms, comprehension checks, and periodic "manual coding" periods prevent skill atrophy and disengagement.

### The Ideal Experience

The developer should spend most of their time in flow — writing code, thinking architecturally, solving creative problems — and periodically surface into a calm, efficient review mode where preattentive visual encoding, progressive disclosure, and intelligent prioritization make evaluation fast and accurate.

The IDE should feel less like an air traffic control console and more like a well-designed cockpit: most information ambient and peripheral, controls consistent and spatial, alerts rare but unmistakable, and the human always the pilot-in-command.

---

## References

Key sources drawn from:
- Sweller — Cognitive Load Theory
- Cowan (2001) — Working memory capacity (4±1 chunks)
- Pirolli & Card — Information Foraging Theory
- Wickens — Multiple Resource Theory / SEEV Model
- Csikszentmihalyi — Flow Theory
- Parnin & DeLine (CHI 2010) — Interrupted programming sessions
- Gloria Mark — Fragmented work and attention residue
- Becker et al. (METR, 2025) — AI coding productivity RCT
- Sheridan — Supervisory Control Framework
- Parasuraman, Sheridan & Wickens (2000) — Levels of Automation
- Bainbridge (1983) — Ironies of Automation
- Lee & See (2004) — Trust in Automation
- Endsley & Kiris (1995) — Out-of-the-loop performance problem
- Treisman — Feature Integration Theory / Preattentive processing
- Epperson et al. (CHI 2025) — AGDebugger multi-agent interaction
- Anthropic (Feb 2026) — Measuring Agent Autonomy
- Weiser & Brown — Calm Technology principles

[Next: UI Design >](UI-Design.md)
