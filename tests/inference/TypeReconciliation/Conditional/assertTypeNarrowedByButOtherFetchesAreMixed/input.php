<?php
/**
 * @return array{0:Exception, ...}
 * @psalm-suppress MixedArgument
 */
function f(array $ret): array {
    assert($ret[0] instanceof Exception);
    echo strlen($ret[1]);
    return $ret;
}
