"""Phase 0 prototype: writes a tiny ONNX model, y = relu(x @ W + b), to test ort and tract.

Run from the repo root inside python/.venv: python python/modele_factice.py
"""

import onnx
from onnx import TensorProto, helper

W = [1.0, -1.0, 0.5, 2.0, -0.5, 1.0, 0.25, -2.0]  # 4 x 2, row-major
B = [0.1, -0.1]

graph = helper.make_graph(
    [
        helper.make_node("MatMul", ["x", "W"], ["xw"]),
        helper.make_node("Add", ["xw", "B"], ["xwb"]),
        helper.make_node("Relu", ["xwb"], ["y"]),
    ],
    "factice",
    [helper.make_tensor_value_info("x", TensorProto.FLOAT, [1, 4])],
    [helper.make_tensor_value_info("y", TensorProto.FLOAT, [1, 2])],
    [
        helper.make_tensor("W", TensorProto.FLOAT, [4, 2], W),
        helper.make_tensor("B", TensorProto.FLOAT, [2], B),
    ],
)
# Opset 17 / IR 8: supported by both ort and tract.
model = helper.make_model(graph, opset_imports=[helper.make_opsetid("", 17)], ir_version=8)
onnx.checker.check_model(model)
onnx.save(model, "crates/lanista_ia_onnx/tests/factice.onnx")
