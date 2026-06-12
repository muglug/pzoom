<?php
function g1(int $i, string $s): bool
{
    $catch_actions = [];
    $catch_actions[$i] = $s;
    return $catch_actions[$i] !== 'END';
}
