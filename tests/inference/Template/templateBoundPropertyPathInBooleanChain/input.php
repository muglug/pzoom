<?php
interface TypeNode {}
class ClassConstantNode implements TypeNode {
    public function __construct(public string $fq_classlike_name) {}
}
class ClassStringNode implements TypeNode {
    public function __construct(public string $as = 'object') {}
}

abstract class NodeVisitor {
    /**
     * @template T as TypeNode
     * @param T $type
     */
    abstract protected function enterNode(TypeNode &$type): ?int;
}

final class ClasslikeReplacer extends NodeVisitor {
    private string $old = 'x';

    protected function enterNode(TypeNode &$type): ?int {
        if ($type instanceof ClassConstantNode) {
            if (strtolower($type->fq_classlike_name) === $this->old) {
                $type = new ClassConstantNode('y');
            }
        } elseif ($type instanceof ClassStringNode) {
            if ($type->as !== 'object' && strtolower($type->as) === $this->old) {
                $type = new ClassStringNode('y');
            }
        }
        return null;
    }
}
