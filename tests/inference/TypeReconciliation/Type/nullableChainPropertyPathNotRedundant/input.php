<?php
class Identifier {
    /** @var non-empty-string */
    public string $name = 'x';
}
class TraitNode {
    public ?Identifier $name = null;
}

function findTrait(TraitNode $node, string $needle): bool {
    /** @psalm-suppress PossiblyNullPropertyFetch */
    if ($node->name->name !== null && strcasecmp($node->name->name, $needle) === 0) {
        return true;
    }
    return false;
}
