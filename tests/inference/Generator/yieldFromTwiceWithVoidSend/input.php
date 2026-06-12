<?php
// this test is all wrong
/**
 * @return \Generator<int, string, void, string>
 */
function test(): \Generator {
    return yield "value";
}

function load(string $rsa_key): \Generator {
    echo (yield from test()) . (yield from test());
    return 5;
}
