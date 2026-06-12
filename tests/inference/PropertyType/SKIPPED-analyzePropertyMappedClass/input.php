<?php
namespace PhpParser\Node\Stmt;

use PhpParser\Node;

class Finally_ extends Node\Stmt
{
    /** @var list<Node\Stmt> Statements */
    public $stmts;

    /**
     * Constructs a finally node.
     *
     * @param list<Node\Stmt> $stmts      Statements
     * @param array<string, mixed>  $attributes Additional attributes
     */
    public function __construct(array $stmts = array(), array $attributes = array()) {
        parent::__construct($attributes);
        $this->stmts = $stmts;
    }

    public function getSubNodeNames() : array {
        return array("stmts");
    }

    public function getType() : string {
        return "Stmt_Finally";
    }
}
