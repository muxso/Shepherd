//! 分页。对应 Java 版到处散落的 PageUtils + PageHelper。
//!
//! 关键:分页参数的**校验规则**是纯函数,本该被单元测试钉死,
//! 而不是等到打到数据库才报错。这正是 Java 版"逻辑长在 IO 上"的典型病灶。

/// 分页请求。构造即校验——非法参数根本造不出来(make illegal states unrepresentable)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageRequest {
    current: u32,   // 第几页,从 1 开始
    page_size: u32, // 每页条数
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageError {
    CurrentMustBePositive,
    PageSizeOutOfRange { max: u32 },
}

impl PageRequest {
    /// 每页上限,防止 `page_size=1_000_000` 拖垮 DB(Java 版常见的未设防点)。
    pub const MAX_PAGE_SIZE: u32 = 500;

    pub fn new(current: u32, page_size: u32) -> Result<Self, PageError> {
        if current < 1 {
            return Err(PageError::CurrentMustBePositive);
        }
        if !(1..=Self::MAX_PAGE_SIZE).contains(&page_size) {
            return Err(PageError::PageSizeOutOfRange { max: Self::MAX_PAGE_SIZE });
        }
        Ok(Self { current, page_size })
    }

    pub fn current(&self) -> u32 {
        self.current
    }

    pub fn page_size(&self) -> u32 {
        self.page_size
    }

    /// SQL OFFSET。`current` 从 1 开始,offset 从 0 开始,差一错(off-by-one)用例必测。
    pub fn offset(&self) -> u64 {
        (self.current as u64 - 1) * self.page_size as u64
    }
}

/// 一页结果。`total_pages` 是派生量,用例锁定取整规则。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Page<T> {
    pub items: Vec<T>,
    pub current: u32,
    pub page_size: u32,
    pub total: u64,
}

impl<T> Page<T> {
    pub fn of(req: PageRequest, total: u64, items: Vec<T>) -> Self {
        Self { items, current: req.current(), page_size: req.page_size(), total }
    }

    pub fn total_pages(&self) -> u64 {
        if self.page_size == 0 {
            return 0;
        }
        self.total.div_ceil(self.page_size as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_zero_current() {
        assert_eq!(PageRequest::new(0, 10), Err(PageError::CurrentMustBePositive));
    }

    #[test]
    fn rejects_zero_page_size() {
        assert_eq!(
            PageRequest::new(1, 0),
            Err(PageError::PageSizeOutOfRange { max: 500 })
        );
    }

    #[test]
    fn rejects_oversized_page_size() {
        assert_eq!(
            PageRequest::new(1, 501),
            Err(PageError::PageSizeOutOfRange { max: 500 })
        );
    }

    #[test]
    fn accepts_boundary_page_size() {
        assert!(PageRequest::new(1, 500).is_ok());
    }

    #[test]
    fn offset_is_zero_on_first_page() {
        let p = PageRequest::new(1, 20).expect("valid");
        assert_eq!(p.offset(), 0);
    }

    #[test]
    fn offset_skips_prior_pages() {
        let p = PageRequest::new(3, 20).expect("valid");
        assert_eq!(p.offset(), 40); // (3-1)*20
    }

    #[test]
    fn total_pages_rounds_up() {
        let req = PageRequest::new(1, 20).expect("valid");
        let page = Page::of(req, 41, Vec::<u8>::new());
        assert_eq!(page.total_pages(), 3); // ceil(41/20)
    }

    #[test]
    fn total_pages_exact_division() {
        let req = PageRequest::new(1, 20).expect("valid");
        let page = Page::of(req, 40, Vec::<u8>::new());
        assert_eq!(page.total_pages(), 2);
    }
}
