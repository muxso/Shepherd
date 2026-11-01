import RequirementOutput from './RequirementOutput'
export default function RequirementExtension({ projectId, title }: { projectId: string; requirementId: string; title: string }) {
  // Match an explicit source serial only; never infer associations from similar titles.
  const reference = title.match(/^\s*([A-Za-z][A-Za-z0-9_]*-\d+)(?:\s|$)/)?.[1] || ''
  return <RequirementOutput projectId={projectId} reference={reference} />
}
