import { useI18n } from '../i18n'
import { PageBody, PageContainer, PageHeader } from '../components/Page'
import WorkbenchSections from '../components/WorkbenchSections'

// Todos = pending items in the selected project, one full-width table per module
// (test plans / case reviews / bugs). Layout and behavior live in WorkbenchSections.
export default function Todos() {
  const { t } = useI18n()
  return (
    <PageContainer>
      <PageHeader title={t('home.todo.title', '待办')} />
      <PageBody>
        <WorkbenchSections mode="todo" />
      </PageBody>
    </PageContainer>
  )
}
