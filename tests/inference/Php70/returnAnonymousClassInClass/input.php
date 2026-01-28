<?php
class A {
    /** @return object */
    public function getNewAnonymousClass() {
        return new class {};
    }
}
