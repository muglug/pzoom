<?php
/**
 * @param string[] $a
 * @return array{b: string, ...}
 */
function foo(array $a) {
    if (isset($a["b"])) {
        return $a;
    }

    throw new \Exception("bad");
}
