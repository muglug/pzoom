<?php
class a {}

/**
 * @return array{test1: 0, test2: 0}|list<a>
 */
function ret() {
    return [new a, new a, new a];
}

$result = ret();
