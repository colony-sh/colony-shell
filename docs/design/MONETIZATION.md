# Colony Monetization Strategy

> Fully functional for free. Pay for convenience, scale, and collaboration.

## Core Philosophy

1. **No login wall** — Colony works completely offline, forever, for free
2. **No feature hostages** — Core shell functionality is never paywalled
3. **Pay for compute, not capabilities** — AI costs money; we pass that through fairly
4. **Teams pay for team features** — Collaboration, admin, compliance
5. **Open source core** — Shell and terminal are open; cloud services are the product

---

## Tier Structure

### 🆓 Free (Local)

**No account required. No internet required. No limits.**

Everything runs locally:

| Feature | Included |
|---------|----------|
| Full POSIX-compatible shell | ✓ |
| Extended channels & buffers | ✓ |
| Tuple space (local) | ✓ |
| Capability system | ✓ |
| colony-terminal (full UI) | ✓ |
| File tree, panels, rich rendering | ✓ |
| Session management & time travel | ✓ |
| Local reflection (rule-based) | ✓ |
| Bring-your-own-API-key for agents | ✓ |
| Unlimited local history | ✓ |
| All future core features | ✓ |

**Local AI option:**
- Users can configure local LLMs (Ollama, llama.cpp, etc.)
- Full agent functionality with no cloud dependency
- Reflection analysis runs locally

This tier is **genuinely complete**. Power users who want to self-host everything can do so forever.

---

### ⭐ Pro ($15/month or $144/year)

**For individuals who want cloud convenience and AI without API key hassle.**

Everything in Free, plus:

| Feature | Detail |
|---------|--------|
| **Colony AI credits** | $10/month in AI usage (Claude, GPT-4, etc.) |
| **Cloud sync** | Settings, history, reflections across devices |
| **Enhanced reflection** | LLM-powered pattern analysis and insights |
| **Cloud tuple spaces** | Personal spaces accessible from anywhere |
| **Priority support** | Email support, community priority |
| **Early access** | Beta features before public release |

**AI Credit System:**
- Credits work across supported models
- Unused credits roll over (up to 3 months)
- Usage dashboard shows spend by feature
- Can add more credits à la carte ($10 = $10 credits)
- BYOK users can use Pro features without credits

**Why this works:**
- $10 in AI credits costs us ~$7-8 (margin on API costs)
- $5/month covers infrastructure + profit
- Users get convenience of "just works" AI
- No surprise bills, predictable cost

---

### 🏢 Team ($25/user/month, minimum 5 users)

**For teams who collaborate on infrastructure, share context, and need oversight.**

Everything in Pro, plus:

| Feature | Detail |
|---------|--------|
| **Shared tuple spaces** | Team-wide coordination spaces |
| **Shared agent definitions** | Team agents with shared context |
| **Shared reflection insights** | Team patterns and learnings (opt-in) |
| **Session sharing** | Live collaboration on terminal sessions |
| **Team history** | Searchable team command/chat history |
| **Usage analytics** | Team-wide AI usage and patterns |
| **Admin dashboard** | User management, access control |
| **Audit logs** | Compliance-ready activity logging |
| **SSO (Google, GitHub, Okta)** | Single sign-on integration |
| **$20/user AI credits** | Higher allocation for team work |

**Collaboration Features:**

```colony
# Share a session live
session share --team
# Teammates can join and see your terminal

# Team tuple space
out @team:deployments { service: "api", version: "2.1", by: $USER }

# Team agent with shared context
agent @team:oncall "What's the status of the api service?"
# Sees team's deployment history, runbooks, past incidents

# Handoff
agent "I'm stuck on this" | handoff @teammate
# Creates shared context they can pick up async
```

---

### 🏛️ Enterprise (Custom pricing)

**For organizations with compliance, security, and scale requirements.**

Everything in Team, plus:

| Feature | Detail |
|---------|--------|
| **SAML/OIDC SSO** | Full enterprise identity integration |
| **SCIM provisioning** | Automated user management |
| **Custom data residency** | Choose where data lives (US, EU, etc.) |
| **Private cloud deployment** | Run Colony cloud in your VPC |
| **On-premise option** | Fully air-gapped deployment |
| **Advanced audit** | SIEM integration, compliance reports |
| **Data retention policies** | Custom retention, legal hold |
| **Priority SLA** | Guaranteed response times |
| **Dedicated support** | Named account manager |
| **Custom integrations** | API access, webhooks, custom agents |
| **Training & onboarding** | Team training sessions |
| **Volume discounts** | Negotiated pricing at scale |

**Security & Compliance:**

```
┌─────────────────────────────────────────────────────────────────┐
│  Enterprise Admin Dashboard                                      │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Security Policies                                              │
│  ├─ Require capability approval for: [fs:/, net:*, exec:*]    │
│  ├─ Block external AI providers: [✓]                           │
│  ├─ Require session recording: [✓]                             │
│  └─ Auto-redact secrets in logs: [✓]                          │
│                                                                 │
│  Agent Policies                                                 │
│  ├─ Approved models: [claude-sonnet, internal-llm]            │
│  ├─ Max context size: 100k tokens                              │
│  ├─ Require human approval for: [write, delete, deploy]       │
│  └─ Custom system prompts: [Enforced]                          │
│                                                                 │
│  Data Policies                                                  │
│  ├─ Retention: 2 years                                         │
│  ├─ Reflection data: Team-aggregate only (no individual)      │
│  ├─ Export: Enabled for compliance                             │
│  └─ Region: EU-West-1                                          │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

---

## Revenue Streams

### Primary Revenue

| Stream | Target | Notes |
|--------|--------|-------|
| Pro subscriptions | Individual developers | Recurring, predictable |
| Team subscriptions | Small-medium teams | Higher ARPU, stickier |
| Enterprise contracts | Large organizations | Annual contracts, services |
| AI credit top-ups | All paid tiers | Usage-based upside |

### Secondary Revenue

| Stream | Model |
|--------|-------|
| Marketplace | Revenue share on premium agent definitions, integrations |
| Training | Paid workshops, certifications |
| Consulting | Enterprise customization, deployment services |
| Support tiers | Premium support packages |

---

## Pricing Rationale

### Pro at $15/month

- **Comparable to:** GitHub Copilot ($19), Warp Teams ($18), Cursor Pro ($20)
- **Includes $10 AI credits:** Users feel they're getting "free" features + AI
- **Annual discount (20%):** Encourages commitment, improves cash flow
- **Low enough for:** Individual developers to expense or self-pay

### Team at $25/user/month

- **Comparable to:** Warp Teams ($22), Slack Pro ($15) + AI tools
- **Value prop:** Collaboration features have clear team ROI
- **Minimum 5 users:** Ensures meaningful team revenue per account
- **Scales with org:** Natural expansion as teams grow

### Enterprise: Custom

- **Starts around:** $40-50/user/month at scale
- **Includes:** Implementation, training, support
- **Annual contracts:** Predictable revenue, lower churn
- **Land and expand:** Start with one team, grow org-wide

---

## Anti-Patterns We Avoid

### ❌ What Warp Got Wrong

| Warp Mistake | Colony Approach |
|--------------|-----------------|
| Mandatory login for basic use | No login required for local use |
| Telemetry concerns | Explicit opt-in, local-first default |
| Closed source | Open source shell + terminal |
| AI features require account | BYOK works fully offline |
| Team features feel bolted on | Collaboration designed from start |

### ❌ What We Won't Do

- **Feature removal:** Free tier never loses features
- **Artificial limits:** No "5 agent queries/day" nonsense
- **Dark patterns:** No "upgrade to continue" interruptions
- **Surprise pricing:** Credits are predictable, no overages
- **Vendor lock-in:** Export everything, take your data

---

## Cloud Architecture

### What Runs in Colony Cloud

```
┌─────────────────────────────────────────────────────────────────┐
│                      Colony Cloud                                │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Identity & Auth          Sync Services        AI Gateway       │
│  ├─ User accounts         ├─ Settings sync     ├─ Model routing │
│  ├─ SSO/SAML             ├─ History sync      ├─ Credit mgmt   │
│  ├─ Team management      ├─ Reflection sync   ├─ Rate limiting │
│  └─ API keys             └─ Tuple spaces      └─ Usage tracking│
│                                                                 │
│  Collaboration            Analytics            Enterprise       │
│  ├─ Session sharing       ├─ Usage metrics     ├─ Audit logs   │
│  ├─ Team spaces          ├─ Reflection agg    ├─ Compliance    │
│  ├─ Handoffs             └─ Growth insights   ├─ Admin APIs    │
│  └─ Async messaging                           └─ SCIM/SAML     │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Data Handling

| Data Type | Free | Pro | Team | Enterprise |
|-----------|------|-----|------|------------|
| Commands | Local only | Sync (encrypted) | Sync + team search | Custom retention |
| Agent chats | Local only | Sync (encrypted) | Sync + team context | Audit + redaction |
| Reflections | Local analysis | Cloud analysis | Team aggregates | Custom policies |
| Tuple spaces | Local only | Personal cloud | Team spaces | Private deployment |
| Credentials | Local keychain | Local keychain | Vault integration | Custom secrets mgmt |

**Encryption:**
- All synced data encrypted at rest (AES-256)
- End-to-end encryption for sensitive data
- User holds keys for personal data
- Org holds keys for team data (admin recovery)

---

## Growth Model

### Phase 1: Community (Year 1)

**Goal:** Build user base, establish trust

- Open source shell + terminal
- Free tier is genuinely excellent
- Build community around Colony
- Content marketing: tutorials, comparisons
- Conference talks, developer relations

**Metrics:**
- GitHub stars, contributors
- Monthly active users (local, no tracking needed for logged-out)
- Community Discord/forum activity

### Phase 2: Monetization (Year 1-2)

**Goal:** Convert power users to Pro

- Launch Pro with cloud sync + AI
- Focus on "time saved" messaging
- Case studies from beta users
- Referral program (give a month, get a month)

**Metrics:**
- Pro conversion rate (target: 2-5% of active users)
- Monthly recurring revenue
- AI credit usage (engagement signal)

### Phase 3: Teams (Year 2-3)

**Goal:** Land team accounts

- Team features launched
- Sales motion for startups, scale-ups
- Integration partnerships (GitHub, GitLab, Slack)
- Team-focused content (collaboration wins)

**Metrics:**
- Team accounts
- Average team size (expansion signal)
- Net revenue retention

### Phase 4: Enterprise (Year 3+)

**Goal:** Enterprise contracts

- Compliance certifications (SOC2, GDPR)
- Enterprise sales team
- Partner channel (consultants, integrators)
- Custom deployment options

**Metrics:**
- Enterprise ARR
- Contract size
- Implementation success rate

---

## Competitive Positioning

```
                    Local-First
                         ↑
                         │
         Colony          │           
         (Free+Pro)      │           
    ●────────────────────┼───────────────────────●
  Open Source            │              Closed Source
         │               │                    │
         │               │              Warp  │
         │               │               ●    │
         │               │                    │
         │               ↓                    │
         │          Cloud-Required            │
         │                                    │
    Fish/Nushell                        iTerm/Terminal
    (no cloud)                          (no AI)
```

**Our position:** Open source foundation, optional cloud that adds genuine value, AI-native without lock-in.

---

## FAQ

**Q: Why not fully open source including cloud?**
A: Cloud services cost money to run. Open source core ensures you're never locked in. Cloud services fund continued development.

**Q: Can I self-host the cloud parts?**
A: Enterprise tier includes private deployment. We may open source cloud components after achieving sustainability.

**Q: What if I exceed my AI credits?**
A: You get a warning at 80%. At 100%, you can: buy more credits, wait for next month, or switch to BYOK. Never cut off mid-task.

**Q: Do you train on my data?**
A: No. Your commands, chats, and reflections are never used for training. Enterprise gets contractual guarantees.

**Q: What happens if Colony shuts down?**
A: Shell and terminal are open source—they keep working. Cloud data is exportable. We commit to 12-month sunset notice.

**Q: Can I pay annually for a bigger discount?**
A: Yes. Annual Pro is $144 (20% off). Annual Team is negotiable. Enterprise is typically annual.

---

## Launch Pricing

### Early Adopter Specials

| Offer | Terms |
|-------|-------|
| **Founding Pro** | $99/year forever (first 1,000 users) |
| **Beta Tester Pro** | 3 months free for active beta feedback |
| **Open Source Contributor** | Pro free while actively contributing |
| **Startup Program** | Team tier free for 1 year (<20 employees, <$5M raised) |

### Price Lock Guarantee

Early adopters keep their price tier as long as subscription stays active. We announce price changes 6 months ahead.

---

## Success Metrics

| Metric | Year 1 | Year 2 | Year 3 |
|--------|--------|--------|--------|
| MAU (all tiers) | 50K | 200K | 500K |
| Pro subscribers | 1K | 10K | 30K |
| Team accounts | - | 200 | 1K |
| Enterprise | - | 10 | 50 |
| ARR | $180K | $3M | $15M |

---

## Summary

Colony monetizes through **genuine value addition**, not artificial scarcity:

- **Free:** Complete, local, forever
- **Pro:** Convenience (sync, managed AI, enhanced reflection)
- **Team:** Collaboration (shared context, team agents, admin)
- **Enterprise:** Compliance (security, control, support)

The shell stays open. The cloud is the product. Users choose their level of involvement.