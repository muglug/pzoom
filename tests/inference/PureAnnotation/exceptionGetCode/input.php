<?php
/**
 * @psalm-pure
 *
 * @return int|string https://www.php.net/manual/en/throwable.getcode.php
 */
function getCode(Throwable $e) {
    return $e->getCode();
}

echo getCode(new Exception("test"));
