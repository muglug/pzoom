<?php
class TFG {}
class DFG {}
class SA {
    public ?TFG $taint_flow_graph = null;
    public TFG|DFG|null $data_flow_graph = null;
}
/** @param list<string> $nodes */
function f(SA $s, array $nodes): void {
    if ($s->taint_flow_graph
        && $nodes
        && !in_array("x", $nodes)
    ) {
        assert($s->data_flow_graph !== null);
        echo "y";
    }
}
