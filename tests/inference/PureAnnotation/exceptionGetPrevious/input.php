<?php
/**
 * @psalm-pure
 */
function getPrevious(Throwable $e): ?Throwable {
    return $e->getPrevious();
}

echo gettype(getPrevious(new Exception("test")));
