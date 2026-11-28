# Decisões HITL pendentes (tríade 2026-11-28)

Três itens do plano exigem **sanção humana** (nunca auto-255). Cada um traz
opções, custo e recomendação. Fechar com `AETHER_HITL_SANCTION_255=1` + token.

---

## D1 — T9: HITL criptográfico (sanção 255 verificável)

**Hoje:** `hitl_sanction_255()` lê `AETHER_HITL_SANCTION_255=1` + token do
corpo/headers. **Não** verifica origem criptograficamente (sem IdP/assinatura).

**Opções:**
1. **Manter local** (status quo documentado como "não criptográfico") — 0 linhas;
   risco: quem controla o host promove 255. Aceitável enquanto `aether-api` é
   localhost.
2. **Token Ed25519 assinado** — host publica pubkey; sanção = JSON assinado
   (request_id + authority + expiração); ~150 linhas; sem dep nova (`ed25519`
   entra no `agent`... ou backend próprio).
3. **IdP externo** (OIDC) — robusto, mas introduz rede/dep grande; só se
   `aether-api` for exposto.

**Recomendação:** opção 2 (2) quando a borda sair de localhost; opção 1 até lá.

---

## D2 — T20: `data/` (265 MB rastreados; `.git` 205 MB)

**Opções:**
1. **git-lfs** para `data/rounds/**` — clone enxuto; exige lfs no CI e nos
   clones existentes (migração de histórico via `git lfs migrate` → **reescreve
   hashes** — precisa acordar o ciclo automático).
2. **Limpeza + regeneração**: `git rm --cached data/rounds/**` (mantém só
   manifests/hash + script de regeneração) + `git gc`. Clone <100 MB.
3. **Status quo** — repo pesado para sempre.

**Recomendação:** opção 2 (sem reescrita de histórico, sem dep de lfs).
Gatilho do script: `data/rounds/*/manifest.json` + hash; regeneração roda os
gates B0/B1.

---

## D3 — T22: upgrades de dependências (sem CVE ativo, mas atrasados)

`wgpu 22→25+` · `eframe/egui 0.29→0.31+` · `rand 0.8→0.9` · `ndarray
0.16→0.17` · `criterion 0.5→0.7`.

**Custo:** upgrades **breaking** (wgpu/eframe exigem re-trabalho do dashboard
e gates GPU no runner dedicado; rand/ndarray tocam o core).

**Recomendação:** fazer **agora** só `ndarray 0.17` + `criterion` (baixo risco,
gates cobrem); `rand 0.9` no próximo ciclo de core; `wgpu`/`eframe` no
próximo ciclo de GPU/dashboard (com runner).

---

## Registro

Fato registrado no nsgdb (auth 255) quando decidido; este doc é o *pacote de
decisão*, não a decisão.
