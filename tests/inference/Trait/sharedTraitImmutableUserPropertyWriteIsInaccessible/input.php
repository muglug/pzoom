<?php
trait CacheIdTrait {
    public function computeId(): string {
        $id = "x";
        $this->cached_id = $id;
        return $id;
    }
}

/** @psalm-immutable */
final class ImmutableNode {
    use CacheIdTrait;
    private ?string $cached_id = null;
    public function __construct(public string $name) {}
}

final class MutableNode {
    use CacheIdTrait;
    private ?string $cached_id = null;
    public function __construct(public string $name) {}
}
