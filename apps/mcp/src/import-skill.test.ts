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

  it('ships three evals, each of which requires not reading the file contents', () => {
    expect(evals.skill_name).toBe('secrethub-import');
    expect(evals.evals).toHaveLength(3);
    expect(evals.evals.map((item) => item.id)).toEqual([1, 2, 3]);
    for (const item of evals.evals) expect(item.expected_output).toMatch(/不读取|拒绝读取|不会进入|不使用|明确拒绝/);
    expect(evals.evals[0].expected_output).toContain('secrethub_request_import');
    expect(evals.evals[0].expected_output).toContain('secrethub_get_import_status');
    expect(evals.evals[1].expected_output).toMatch(/拒绝读取或展示/);
    expect(evals.evals[2].expected_output).toMatch(/拒绝提供任何密码/);
  });
});
