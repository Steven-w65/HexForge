import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import StatusBar from './StatusBar.vue'

const statusProps = (selected: { offset: bigint; value: number } | null = null) => ({
  fileSize: 4096n, selected, selectedCount: selected ? 1n : 0n, bytesPerRow: 16 as const,
  editMode: false, endianness: 'little' as const, dirty: false,
})

describe('StatusBar', () => {
  it('renders printable and non-printable selected bytes', async () => {
    const wrapper = mount(StatusBar, { props: statusProps({ offset: 1n, value: 0xff }) })
    expect(wrapper.text()).toContain('FF')
    expect(wrapper.text()).toContain('.')
    await wrapper.setProps(statusProps({ offset: 2n, value: 0x41 }))
    expect(wrapper.text()).toContain('A')
  })

  it('shows all state and emits row-width changes', async () => {
    const wrapper = mount(StatusBar, { props: { ...statusProps(), dirty: true, editMode: true, endianness: 'big' as const, busyLabel: 'Saving copy', progressText: '64 / 128' } })
    expect(wrapper.text()).toContain('4 KiB')
    expect(wrapper.text()).toContain('Edit')
    expect(wrapper.text()).toContain('Parse: BE')
    expect(wrapper.text()).toContain('Modified')
    expect(wrapper.get('[data-testid="status-progress"]').text()).toContain('Saving copy · 64 / 128')
    await wrapper.get('[data-row-width="32"]').trigger('click')
    expect(wrapper.emitted('update:bytesPerRow')).toEqual([[32]])
  })
})
