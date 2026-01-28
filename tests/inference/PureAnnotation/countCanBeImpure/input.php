<?php
class A implements Countable {
    public function count(): int {
        echo "oops";
        return 2;
    }
}

/**
 * @psalm-pure
 */
function thePurest(A $countable): int {
    return count($countable);
}
