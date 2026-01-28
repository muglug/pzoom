<?php
/**
 * @param mixed $req
 * @param mixed $opt
 * @param mixed ...$params
 * @return array<mixed>
 */
function f($req, $opt = null, ...$params) {
    return $params;
}

f(1);
f(1, 2);
f(1, 2, 3);
f(1, 2, 3, 4);
f(1, 2, 3, 4, 5);
