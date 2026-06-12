<?php

/**
 * @return Generator<string, int, void, void>
 */
function test(): Generator {
    yield 'test' => 1;
}

foreach (test() as $_) {
}
