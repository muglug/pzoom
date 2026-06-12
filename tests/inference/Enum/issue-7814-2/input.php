<?php
enum State
{
    case A;
    case B;
    case C;
}

/**
 * @template T of State
 */
final class WithState
{
    /**
     * @param T $s
     */
    public function __construct(
        public readonly State $s,
    ) {}
}

/**
 * @param WithState<State::A> $_
 */
function withA(WithState $_): void {}

// Should be issue here. But nothing
// Argument 1 of withA expects WithState<enum(State::A)>, WithState<enum(State::C)> provided
$withstate_c = new WithState(State::C);
withA($withstate_c);
                
