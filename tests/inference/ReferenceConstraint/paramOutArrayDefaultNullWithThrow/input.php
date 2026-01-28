<?php
/**
 * @param-out array{errors: int}|null $info
 */
function idnToAsci(?array &$info = null): void {
    if (rand(0, 1)) {
        $info = null;
    }

    throw new \UnexpectedValueException();
}
