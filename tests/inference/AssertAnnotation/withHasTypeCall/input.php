<?php
/**
 * @psalm-immutable
 */
class Param {
    /**
     * @psalm-assert-if-true ReflectionType $this->getType()
     */
    public function hasType() : bool {
        return true;
    }

    public function getType() : ?ReflectionType {
        return null;
    }
}

function takesParam(Param $p) : void {
    if ($p->hasType()) {
        echo $p->getType()->__toString();
    }
}
