<?php
/** @return array{0:Exception, ...} */
function f(array $a): array {
    if ($a[0] instanceof Exception) {
        return $a;
    }

    return [new Exception("bad")];
}
