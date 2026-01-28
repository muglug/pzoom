<?php
/**
 * @psalm-taint-escape
 */
function takesInt(int $i): int {
    return $i;
}
