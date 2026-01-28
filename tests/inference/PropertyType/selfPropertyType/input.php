<?php
class Node
{
    /** @var self|null */
    public $next;

    public function __construct() {
        if (rand(0, 1)) {
            $this->next = new Node();
        }
    }
}

$node = new Node();
$next = $node->next;
