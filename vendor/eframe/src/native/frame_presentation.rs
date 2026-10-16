//! Ordering boundary shared with HexForge's native regression harness.
//! Drawing into the back buffer is not presentation. A hidden new window
//! must stay hidden until swapping that real frame succeeds.

pub(super) fn swap_then_reveal<E>(
    swap: impl FnOnce() -> Result<(), E>,
    reveal: impl FnOnce(),
) -> Result<(), E> {
    // The Windows compositor must never see the old/empty front buffer of a
    // new root window. Preserve the renderer's error if the swap fails, and
    // leave startup error handling to the existing caller without revealing it.
    swap()?;
    reveal();
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn root_is_revealed_only_after_successful_swap() {
        let calls = std::cell::RefCell::new(Vec::new());
        let result = super::swap_then_reveal(
            || {
                calls.borrow_mut().push("swap completed");
                Ok::<_, &str>(())
            },
            || calls.borrow_mut().push("reveal actual scene"),
        );
        assert_eq!(result, Ok(()));
        assert_eq!(*calls.borrow(), ["swap completed", "reveal actual scene"]);
    }

    #[test]
    fn failed_swap_never_reveals_an_unpresented_window() {
        let calls = std::cell::RefCell::new(Vec::new());
        let result = super::swap_then_reveal(
            || {
                calls.borrow_mut().push("swap failed");
                Err("surface unavailable")
            },
            || calls.borrow_mut().push("reveal actual scene"),
        );
        assert_eq!(result, Err("surface unavailable"));
        assert_eq!(*calls.borrow(), ["swap failed"]);
    }
}
