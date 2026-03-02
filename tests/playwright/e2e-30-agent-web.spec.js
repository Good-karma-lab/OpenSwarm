const { test, expect } = require('@playwright/test')

async function waitFor(check, timeoutMs = 60000, stepMs = 1000) {
  const start = Date.now()
  for (;;) {
    const result = await check()
    if (result) return result
    if (Date.now() - start > timeoutMs) {
      throw new Error('Condition not met before timeout')
    }
    await new Promise((r) => setTimeout(r, stepMs))
  }
}

test('30-agent swarm web console shows requested capabilities', async ({ page }) => {
  test.setTimeout(300000)
  page.on('crash', () => {
    throw new Error('Browser page crashed during 30-agent web E2E')
  })

  await page.goto('/')
  await expect(page.getByText('WWS')).toBeVisible()

  // 1) Expandable hierarchy
  await page.getByRole('button', { name: 'hierarchy' }).click()
  await expect(page.getByText('Expandable Hierarchy')).toBeVisible()

  const hierarchy = await waitFor(async () => {
    const resp = await page.request.get('/api/hierarchy')
    const payload = await resp.json()
    return payload.nodes?.length > 0 ? payload : null
  }, 120000)
  const totalNodes = hierarchy.nodes.length
  const tier1 = hierarchy.nodes.filter((n) => n.tier === 'Tier1').length
  const tier2 = hierarchy.nodes.filter((n) => n.tier === 'Tier2').length
  if (totalNodes > 10) {
    const expectedTier1 = 10
    const expectedTier2 = totalNodes - expectedTier1
    expect(tier1).toBe(expectedTier1)
    expect(tier2).toBe(expectedTier2)
  }
  // 4) Submit task from UI
  const taskText = `Playwright real e2e task ${Date.now()}`
  await page.locator('textarea').first().fill(taskText)
  await page.getByRole('button', { name: 'Submit' }).click()

  const tasksResp = await page.request.get('/api/tasks')
  const tasksPayload = await tasksResp.json()
  const submitted = (tasksPayload.tasks || []).find((t) => (t.description || '').includes(taskText))
  expect(submitted).toBeTruthy()

  // 2) Voting logs
  await page.getByRole('button', { name: 'voting' }).click()
  await expect(page.getByText('Voting Process Logs')).toBeVisible()
  const votingResp = await page.request.get('/api/voting')
  const votingPayload = await votingResp.json()
  expect(Array.isArray(votingPayload.rfp)).toBeTruthy()
  expect(Array.isArray(votingPayload.voting)).toBeTruthy()
  await expect(page.locator('table')).toBeVisible()

  // 3) P2P message logs
  await page.getByRole('button', { name: 'messages' }).click()
  await expect(page.getByText('Peer-to-Peer Debug Logs')).toBeVisible()
  const messages = await waitFor(async () => {
    const resp = await page.request.get('/api/messages')
    const payload = await resp.json()
    return payload.length > 0 ? payload : null
  })
  expect(messages.length).toBeGreaterThan(0)
  await expect(page.locator('.log.mono > div').first()).toBeVisible()

  // 5) Task forensics panel
  await page.getByRole('button', { name: 'task' }).click()
  await page.getByPlaceholder('task id').fill(submitted.task_id)
  await page.getByRole('button', { name: 'Load Timeline' }).click()
  await expect(page.getByText('Task Timeline Replay')).toBeVisible()
  await expect(page.getByText('Task DAG')).toBeVisible()
  await expect(page.getByText('Root Task + Aggregation State')).toBeVisible()
  const taskTimeline = await waitFor(async () => {
    const resp = await page.request.get(`/api/tasks/${submitted.task_id}/timeline`)
    const payload = await resp.json()
    return (payload.timeline || []).length > 0 ? payload : null
  })
  expect(taskTimeline.timeline.some((e) => e.stage === 'injected')).toBeTruthy()
  await expect(page.locator('.log.mono').first()).toContainText('injected')

  // 6) Topology visualization
  await page.getByRole('button', { name: 'topology' }).click()
  await expect(page.locator('#topologyGraph')).toBeVisible()
  const topologyResp = await page.request.get('/api/topology')
  const topology = await topologyResp.json()
  expect(topology.nodes.some((n) => n.name && n.name.length > 0)).toBeTruthy()

  // 7) UI feature recommendations
  await page.getByRole('button', { name: 'ideas' }).click()
  await expect(page.getByText('Proposed Next UI Features')).toBeVisible()

  // Audit visibility
  await page.getByRole('button', { name: 'audit' }).click()
  await expect(page.getByText('Operator Audit Log')).toBeVisible()
  await expect(page.locator('.log.mono')).toContainText('AUDIT web.submit_task')
})
