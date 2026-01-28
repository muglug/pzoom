<?php
/** @template TData */
class Container {
    /** @var TData */
    public $data;

    /** @param TData $data */
    public function __construct($data) {
        $this->data = $data;
    }
}

/** @param Container<string> $r */
function takesContainer(Container $r): void {
    $r->data = "David";
}

$me = new Container("Matthew");

takesContainer($me);

if ($me->data === "David") {}