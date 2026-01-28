<?php
class Node {
    /** @var Node|null */
    public $next;
}

/** @return void */
function test(Node $head) {
    for ($node = $head; $node; $node = $next) {
        $next = $node->next;
        $node->next = null;
    }
}
