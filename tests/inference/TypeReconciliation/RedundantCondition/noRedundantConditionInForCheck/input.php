<?php
class Node
{
    /** @var Node|null */
    public $next;

    public function iterate(): void
    {
        for ($node = $this; $node !== null; $node = $node->next) {}
    }
}