<?php
class NewA {}
class_alias(NewA::class, OldA::class);
function action(NewA $_m): void {}

class OldAChild extends OldA {}
action(new OldA());
action(new OldAChild());
