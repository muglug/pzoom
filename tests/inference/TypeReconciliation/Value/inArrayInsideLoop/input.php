<?php
class A {
    const ACTION_ONE = "one";
    const ACTION_TWO = "two";
    const ACTION_THREE = "two";
}

while (rand(0, 1)) {
    /** @var list<A::ACTION_*> */
    $case_actions = [];

    if (!in_array(A::ACTION_ONE, $case_actions, true)) {}
}