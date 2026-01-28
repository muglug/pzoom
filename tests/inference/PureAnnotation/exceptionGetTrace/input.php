<?php
/**
 * @psalm-pure
 */
function getTrace(Throwable $e): array {
    return $e->getTrace();
}

echo count(getTrace(new Exception("test")));
