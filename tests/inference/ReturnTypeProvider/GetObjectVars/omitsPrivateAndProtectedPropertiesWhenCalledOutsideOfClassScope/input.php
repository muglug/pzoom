<?php
final class C {
    /** @var string */
    private $priv = "val";

    /** @var string */
    protected $prot = "val";
}
$ret = get_object_vars(new C);
