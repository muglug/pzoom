<?php

abstract class Node {
    abstract public function compute(): int;
}

trait SummingTrait {
    public function total(): int {
        $sum = 0;
        foreach ($this->nodes as $node) {
            $sum += $node->compute();
        }
        return $sum;
    }
}

final class Tree {
    use SummingTrait;

    /** @var list<Node> */
    private array $nodes;

    /** @param list<Node> $nodes */
    public function __construct(array $nodes) {
        $this->nodes = $nodes;
    }
}

final class Leaf extends Node {
    #[Override]
    public function compute(): int {
        return 1;
    }
}

echo (new Tree([new Leaf()]))->total();
