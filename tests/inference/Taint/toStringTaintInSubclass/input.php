<?php // --taint-analysis
class TaintedBaseClass {
    /** @psalm-taint-source input */
    public function __toString() {
        return "x";
    }
}
class TaintedSubclass extends TaintedBaseClass {}
$x = new TaintedSubclass();
echo "Caught: $x\n";
