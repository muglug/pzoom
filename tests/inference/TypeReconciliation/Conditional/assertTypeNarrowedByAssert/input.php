<?php
/** @return array{0:Exception,1:Exception, ...} */
function f(array $ret): array {
    assert($ret[0] instanceof Exception);
    assert($ret[1] instanceof Exception);
    return $ret;
}
