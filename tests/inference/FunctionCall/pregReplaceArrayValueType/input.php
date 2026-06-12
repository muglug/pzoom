<?php
/**
 * @param string[] $s
 * @return string[]
 */
function foo($s): array {
    return preg_replace("/hello/", "", $s);
}
