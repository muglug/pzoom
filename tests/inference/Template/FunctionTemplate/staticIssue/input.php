<?php

/**
 * @template TTKey as array-key
 * @template TTValue
 *
 * @psalm-consistent-constructor
 * @psalm-consistent-templates
 */
final class a {

    /**
     * @var array<TTKey, TTValue>
     */
    protected array $array = [];

    /**
     * Constructor
     *
     * @param array<TTKey, TTValue> $array
     */
    public function __construct(array $array = []) {
        $this->array = $array;
    }

    /**
     * @template TNewValue
     * @param TNewValue $item
     *
     * @psalm-self-out a<TTKey|int, TTValue|TNewValue>
     * @return self<TTKey|int, TTValue|TNewValue>
     */
    public function add($item): self {
        /** @var self<TTKey|int, TTValue|TNewValue> $this */
        $this->array[] = $item;
        return $this;
    }

    /**
     * @template TRet as list<int|string|float>
     * @param callable(TTValue, TTKey): TRet $func
     *
     * @return self<TTKey, TTValue>
     */
    public function sortByArr(callable $func, int $order = -1): self {
        return new self($this->array);
    }
}


class FilterBase {
    public function setId(): static {
        return $this;
    }
}

class FilterControls extends FilterBase {
}


$list = new a();
$list->add(
    (new FilterControls())->setId(),
);

$test = $list->sortByArr(
    /** 
     * @return list{0: mixed, 1: mixed}
     */
    static fn ($item): array => [$item, $item],
    1,
);
