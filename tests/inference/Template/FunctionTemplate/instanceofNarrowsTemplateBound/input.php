<?php
abstract class NodeB {}
class NullableTypeB extends NodeB {
    public NodeB $type;
    public function __construct(NodeB $t) { $this->type = $t; }
}

class ResolverB {
    /**
     * @template T of NodeB|null
     * @param T $node
     * @return T
     */
    private function resolveType(?NodeB $node): ?NodeB
    {
        if ($node instanceof NullableTypeB) {
            $node->type = $node->type;
            return $node;
        }
        return $node;
    }
}
