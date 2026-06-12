<?php
trait T {
    /**
     * @param mixed $instance
     * @return ?static
     */
    public static function filterInstance($instance) {
        return $instance instanceof static ? $instance : null;
    }
}

class A {
    use T;
}
