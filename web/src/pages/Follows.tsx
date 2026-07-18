import { useI18n } from '../i18n'
import { PageBody, PageContainer, PageHeader } from '../components/Page'
import WorkbenchSections from '../components/WorkbenchSections'

// Follows = assets I watch, six sections: plans (local follow set); functional
// cases, case reviews, API cases/scenarios and bugs via /follow/mine per
// entity type. Shared layout in WorkbenchSections.
export default function Follows() {
  const { t } = useI18n()
  return (
    <PageContainer>
      <PageHeader title={t('home.follow.title', '关注')} />
      <PageBody>
        <WorkbenchSections mode="follow" />
      </PageBody>
    </PageContainer>
  )
}
