<?php
interface TypeNode
{
    /**
     * @param static $node
     * @param-out static $node
     */
    public static function visitMutable(Visitor $visitor, &$node, bool $cloned): bool;
}

final class Visitor {}

final class U implements TypeNode
{
    /** @var list<int> */
    public array $types = [];

    public static function visitMutable(Visitor $visitor, &$node, bool $cloned): bool
    {
        $types = $node->types;
        foreach ($types as $type) {
            echo $type;
        }
        return true;
    }
}
