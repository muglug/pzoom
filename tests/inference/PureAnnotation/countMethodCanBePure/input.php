<?php
class A implements Countable {
    /** @psalm-mutation-free */
    public function count(): int {
        return 2;
    }
}

/**
 * @psalm-pure
 */
function thePurest(A $countable): int {
    return count($countable);
}
