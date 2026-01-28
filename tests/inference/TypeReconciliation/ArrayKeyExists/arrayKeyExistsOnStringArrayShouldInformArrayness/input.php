<?php
/**
 * @param string[] $a
 * @return array{b: string, ...}
 */
function foo(array $a) {
    if (array_key_exists("b", $a)) {
        return $a;
    }

    throw new \Exception("bad");
}
