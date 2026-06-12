<?php
final class C {
    /** @var string */
    private $priv = "val";

    /** @var string */
    protected $prot = "val";

    /** @return array{priv: string, prot: string} */
    public function method(): array {
        return get_object_vars($this);
    }
}
