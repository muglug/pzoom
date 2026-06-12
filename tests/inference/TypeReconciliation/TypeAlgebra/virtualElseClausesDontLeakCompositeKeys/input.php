<?php
abstract class Node {}
class NameNode extends Node {
    public function getFirst(): string { return 'x'; }
}
class IdentifierNode extends Node { public string $name = ''; }
class FuncCallNode extends Node {
    public Node $name;
    public function __construct(Node $n) { $this->name = $n; }
}
class ClassConstFetchNode extends Node {
    public Node $name;
    public function __construct(Node $n) { $this->name = $n; }
}
class BinaryNode { public Node $left; public function __construct(Node $l) { $this->left = $l; } }

function hasGetClassCheck(BinaryNode $conditional): bool {
    $left_class_const = $conditional->left instanceof ClassConstFetchNode
        && $conditional->left->name instanceof IdentifierNode;

    if (!$left_class_const) {
        echo "x";
    }

    $left_get_class = $conditional->left instanceof FuncCallNode
        && $conditional->left->name instanceof NameNode
        && strtolower($conditional->left->name->getFirst()) === 'get_class';

    return $left_get_class;
}
