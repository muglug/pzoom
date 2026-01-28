<?php
/**
 * @template P as string
 * @template V as mixed
 *
 * @psalm-no-seal-properties
 */
class PropertyBag {
    /** @var array<P,V> */
    protected $data = [];

    /** @param array<P,V> $data */
    public function __construct(array $data) {
        $this->data = $data;
    }

    /** @param P $name */
    public function __isset(string $name): bool {
        return isset($this->data[$name]);
    }

    /**
     * @param P $name
     * @return V
     */
    public function __get(string $name) {
        return $this->data[$name];
    }
}

$p = new PropertyBag(["a" => "data for a", "b" => "data for b"]);

$a = $p->a;
