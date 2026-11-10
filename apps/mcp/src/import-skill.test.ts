import { readFileSync } from 'node:fs';

const skillDirectory = new URL('../../../skills/secrethub-import/', import.meta.url);
const skill = readFileSync(new URL('SKILL.md', skillDirectory), 'utf8');
const evals = JSON.parse(readFileSync(new URL('evals/evals.json', skillDirectory), 'utf8')) as { skill_name: string; evals: Array<{ id: number; prompt: string; expected_output: string }> };

describe('secrethub-import skill', () => {
  it('has front matter that names the skill and describes when to use it', () => {
    expect(skill).toMatch(/^---\nname: secrethub-import\n/);
    expect(skill).toMatch(/description: >-/);
    expect(skill).toMatch(/compatibility:/);
  });

  it('forbids reading file contents and routes through the import tools', () => {
    expect(skill).toMatch(/Never read the contents of the files/);
    expect(skill).toMatch(/Read, cat, head, grep/);
    expect(skill).toMatch(/\.env/);
    expect(skill).toMatch(/id_ed25519/);
    expect(skill).toContain('secrethub_request_import');
    expect(skill).toContain('secrethub_get_import_status');
    expect(skill).toMatch(/names only/i);
    expect(skill).toMatch(/Desktop/);
  });

  it('makes the masked-view flow the default and sets the organizing rules', () => {
    for (const tool of ['secrethub_import_inspect', 'secrethub_get_import_inspection', 'secrethub_apply_import_plan']) expect(skill, tool).toContain(tool);
    expect(skill.indexOf('secrethub_import_inspect')).toBeLessThan(skill.indexOf('secrethub_request_import'));
    expect(skill).toMatch(/One group per server or service/);
    expect(skill).toMatch(/UPPER_CASE, with a prefix/);
    expect(skill).toMatch(/Describe every variable/);
    expect(skill).toMatch(/Flag doubts/);
    expect(skill).toMatch(/BT \(宝塔\) panel/);
    expect(skill).toMatch(/\{ "token": "S3" \}/);
    expect(skill).toMatch(/Never guess,\s+invent or "complete" a password/);
    expect(skill).toMatch(/must never contain a value/);
    expect(skill).toMatch(/撤销此次导入/);
    expect(skill).toMatch(/When a project later fails to connect/);
    expect(skill).toMatch(/可能不对/);
  });

  it('ships five evals, each of which requires not reading the file contents', () => {
    expect(evals.skill_name).toBe('secrethub-import');
    expect(evals.evals).toHaveLength(5);
    expect(evals.evals.map((item) => item.id)).toEqual([1, 2, 3, 4, 5]);
    for (const item of evals.evals) expect(item.expected_output).toMatch(/不读取|拒绝读取|不会进入|不使用|明确拒绝/);
    for (const tool of ['secrethub_import_inspect', 'secrethub_get_import_inspection', 'secrethub_apply_import_plan', 'secrethub_get_import_status']) expect(evals.evals[0].expected_output).toContain(tool);
    expect(evals.evals[0].expected_output).toContain('撤销');
    expect(evals.evals[3].expected_output).toMatch(/拒绝猜测或编造密码/);
    expect(evals.evals[4].expected_output).toMatch(/可能不对/);
    expect(evals.evals[1].expected_output).toMatch(/拒绝读取或展示/);
    expect(evals.evals[2].expected_output).toMatch(/拒绝提供任何密码/);
  });
});
