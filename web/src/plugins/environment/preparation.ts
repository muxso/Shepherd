// Native cases keep generic profile references and phase tags; provider data stays in plugins.
export function phaseProcessors(pre: unknown[] = [], post: unknown[] = []) {
  const tag = (list: unknown[], phase: 'pre' | 'post') => list.map(value => {
    const processor = value as { args?: Record<string, unknown> }
    return { ...processor, args: { ...(processor.args || {}), phase } }
  })
  return [...tag(pre, 'pre'), ...tag(post, 'post')]
}
export function splitProcessors(processors: unknown[] = []) {
  const preProcessors: unknown[] = [], postProcessors: unknown[] = []
  for (const value of processors) {
    const processor = value as { type?: string; phase?: string; args?: { phase?: string } }
    const phase = processor.phase ?? processor.args?.phase ?? (processor.type === 'Extract' ? 'post' : 'pre')
    ;(phase === 'pre' ? preProcessors : postProcessors).push(processor)
  }
  return { preProcessors, processors: postProcessors }
}
