<?php

/** @psalm-yield int */
class a {}

/**
 * @return Generator<int, a, mixed, int>
 */
function a(): Generator {
    return random_int(0, 1) ? 123 : yield new a;
}
