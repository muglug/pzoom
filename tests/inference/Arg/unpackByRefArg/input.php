<?php
function example (int &...$x): void {}
$y = 0;
example($y);
$z = [0];
example(...$z);
