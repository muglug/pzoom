<?php
/**
 * @param array{x:int,y:int}|null $start_pos
 * @return array{x:int,y:int}|null
 */
function foo(?array $start_pos) : ?array {
    if ($start_pos) {}

    return $start_pos;
}