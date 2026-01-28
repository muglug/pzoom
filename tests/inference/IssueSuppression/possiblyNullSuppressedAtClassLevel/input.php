<?php
/** @psalm-suppress PossiblyNullReference */
class C {
    private ?DateTime $mightBeNull = null;

    public function m(): string {
        return $this->mightBeNull->format("");
    }
}
                
