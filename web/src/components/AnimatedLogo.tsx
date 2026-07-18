import './AnimatedLogo.css'

/**
 * Shepherd mark with motion: the sheared strata slide into place once on
 * mount, then a subtle shimmer loops through the layers like execution
 * traces flowing across the hand-off seam. Colors follow currentColor.
 */
export default function AnimatedLogo({ size = 96, className }: { size?: number; className?: string }) {
  return (
    <svg
      className={`shepherd-logo${className ? ` ${className}` : ''}`}
      xmlns="http://www.w3.org/2000/svg"
      viewBox="0 0 1024 1024"
      width={size}
      height={size}
      aria-label="Shepherd"
    >
      <g fill="currentColor" transform="translate(20,115)">
        <polygon className="stratum s-1" points="310,0 850,0 810,40 310,40" />
        <polygon className="stratum s-2" points="310,73 777,73 737,113 310,113" />
        <polygon className="stratum s-3" points="310,146 704,146 664,186 310,186" />
        <polygon className="stratum s-4" points="130,186 170,186 170,316 130,276" />
        <polygon className="stratum s-5" points="203,186 243,186 243,386 203,346" />
        <path className="stratum s-6" d="M270 186 L310 186 L310 337 L653 337 L653 377 L310 377 L310 417 L653 417 L653 457 L310 457 L270 417 Z" />
        <polygon className="stratum s-7" points="814,337 854,337 854,617 814,657" />
        <polygon className="stratum s-8" points="744,337 784,337 784,687 744,727" />
        <path className="stratum s-9" d="M674 337 L714 337 L714 754 L674 794 L132 794 L174 754 L674 754 L674 722 L205 722 L247 682 L674 682 L674 648 L278 648 L320 608 L674 608 Z" />
      </g>
    </svg>
  )
}
