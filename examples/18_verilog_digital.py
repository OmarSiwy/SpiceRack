"""
Example 18: Digital Verilog Co-simulation (XSPICE)

4-bit counter in Verilog bridged to analog via XSPICE A-elements.
Demonstrates A element, adc_bridge/dac_bridge models, and raw_spice
for the d_cosim connection.

Requires ngspice with XSPICE + iverilog on $PATH.
"""
import pyspice_rs as ps
from pyspice_rs.unit import u_V

VERILOG_COUNTER = """\
module counter(clk, rst, count);
    input clk, rst;
    output [3:0] count;
    reg [3:0] count;
    always @(posedge clk or posedge rst) begin
        if (rst) count <= 4'b0000;
        else     count <= count + 1;
    end
endmodule
"""

# ── DUT ──

dut = ps.Subcircuit("cosim_demo", ["vdd", "clk", "rst",
                                    "acount0", "acount1", "acount2", "acount3"])

dut.model("adc_bridge_model", "adc_bridge", in_low=0.8, in_high=2.0)
dut.model("dac_bridge_model", "dac_bridge", out_low=0.0, out_high=3.3)

dut.A(name="adc1", connections=["[clk rst]", "[dclk drst]"], model="adc_bridge_model")

dut.raw_spice("* Digital counter (requires iverilog + ngspice XSPICE)")
dut.raw_spice("* .model counter_model d_cosim(delay=0.1n)")
dut.raw_spice("* A_counter [dclk drst] [dcount0 dcount1 dcount2 dcount3] counter_model")

dut.A(name="dac1",
      connections=["[dcount0 dcount1 dcount2 dcount3]",
                   "[acount0 acount1 acount2 acount3]"],
      model="dac_bridge_model")

for i in range(4):
    dut.R(name=f"out{i}", positive=f"acount{i}", negative=dut.gnd, value=10e3)

# ── Testbench ──

tb = ps.Testbench(dut)
tb.V(name="dd", positive="vdd", negative=dut.gnd, value=3.3 @ u_V)
tb.PulseVoltageSource(
    name="clk", positive="clk", negative=dut.gnd,
    pulsed_value=3.3, pulse_width=5e-9, period=10e-9,
    rise_time=0.1e-9, fall_time=0.1e-9,
)
tb.PulseVoltageSource(
    name="rst", positive="rst", negative=dut.gnd,
    initial_value=3.3, pulsed_value=0.0,
    pulse_width=1.0, period=2.0,
    rise_time=0.1e-9, fall_time=0.1e-9,
)
tb.with_backend("ngspice")

print("XSPICE co-simulation testbench built.")
print(f"Netlist:\n{tb}")
print(f"\nVerilog source:\n{VERILOG_COUNTER}")
print("Requires: ngspice w/ XSPICE, iverilog, d_cosim model")
