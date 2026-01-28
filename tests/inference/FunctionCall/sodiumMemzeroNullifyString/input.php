<?php
function returnsStr(): string {
    $str = "x";
    sodium_memzero($str);
    return $str;
}
