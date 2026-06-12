<?php
class PureCountable implements \Countable {
    /** @psalm-pure */
    public function count(): int { return 1; }
}
/** @psalm-pure */
function example(PureCountable $x) : int {
    return count($x);
}
