<?php
/** @psalm-param list<string> $keys */
function &ensure_array(array &$what, array $keys): array
{
    $arr = & $what;
    while ($key = array_shift($keys)) {
        if (!isset($arr[$key]) || !is_array($arr[$key])) {
            $arr[$key] = array();
        }
        $arr = & $arr[$key];
    }
    return $arr;
}
                
