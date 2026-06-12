<?php
final class U2 {}

final class K {
    /** @var non-empty-array<int|string, U2> */
    public array $properties;

    public bool $is_list = false;

    public function __construct(U2 $u) { $this->properties = [$u]; }

    /**
     * @psalm-assert-if-true list{U2} $this->properties
     */
    public function isGenericList(): bool {
        return $this->is_list && count($this->properties) === 1;
    }

    public function getId(): string {
        if ($this->isGenericList()) {
            $first = $this->properties[0];
            return get_class($first);
        }
        return 'x';
    }
}
