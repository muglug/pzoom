<?php
class a {}

/**
 * @return list<a>|list{0, 0}
 */
function ret() {
    return [new a, new a, new a];
}

$result = ret();
