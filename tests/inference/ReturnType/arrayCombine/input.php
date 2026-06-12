<?php
class a {}

/**
 * @return list{0, 0}|list<a>
 */
function ret() {
    return [new a, new a, new a];
}

$result = ret();
