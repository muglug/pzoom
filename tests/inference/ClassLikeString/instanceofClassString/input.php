<?php
function f(Exception $e): ?InvalidArgumentException {
    $type = InvalidArgumentException::class;
    if ($e instanceof $type) {
        return $e;
    } else {
        return null;
    }
}
