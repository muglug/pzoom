<?php
namespace Bar;

/** @psalm-pure */
function lower(string $s) : string {
    return substr(strtolower($s), 0, 10);
}
