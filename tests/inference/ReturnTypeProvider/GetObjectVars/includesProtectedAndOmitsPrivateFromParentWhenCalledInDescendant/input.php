<?php
class C {
    /** @var string */
    private $priv = "val";

    /** @var string */
    protected $prot = "val";

    /** @var string */
    public $pub = "val";
}

final class D extends C {
    /** @return array{prot: string, pub: string} */
    public function method(): array {
        return get_object_vars($this);
    }
}
