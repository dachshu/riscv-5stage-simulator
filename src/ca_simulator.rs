//! Cycle accurate 5-stage pipelining RISC-V 32I simulator.


use consts;
use hazards::{ex_hazard_src1, ex_hazard_src2, mem_hazard_src1,
              mem_hazard_src2, stall};
use instruction::{Function, Instruction, Opcode};
use memory::data::DataMemory;
use memory::instruction::InstructionMemory;
use pipeline::{IfIdRegister, IdExRegister, ExMemRegister, MemWbRegister};
use register::RegisterFile;
use stages;


/// Runs a cycle accurate RISC-V 32I simulator.
///
/// Returns the PC address of the HALT instruction.
///
pub fn run(instructions: &InstructionMemory) -> u32 {
    let mut mem = DataMemory::new(1024);
    let mut reg = RegisterFile::new(0x0);

    // Pipline registers
    let mut if_id = IfIdRegister::new();
    let mut id_ex = IdExRegister::new();
    let mut ex_mem = ExMemRegister::new();
    let mut mem_wb = MemWbRegister::new();

    // clock -> rising edge, !clock -> falling edge
    let mut clock: bool = true;
    let mut bubble: bool = false;

    loop {
        if clock {
            if bubble {
                bubble = false;
            } else {
                // Read and increment program counter
                let pc = reg.pc.read() as usize;
                let npc = (pc + consts::WORD_SIZE) as u32;
                reg.pc.write(npc);

                // IF: Instruction fetch
                let raw_insn = stages::insn_fetch(instructions, pc);

                if_id.npc = npc;
                if_id.raw_insn = raw_insn;
            }
        } else {
            // ID: Instruction decode and register file read
            let raw_insn = if_id.raw_insn;
            let insn = stages::insn_decode(raw_insn);

            if stall(if_id, id_ex) {
                bubble = true;
                id_ex.insn = Instruction::default(); // NOP
            } else {
                id_ex.npc = if_id.npc;
                id_ex.insn = insn;

                let (rs1, rs2) = stages::reg_read(&insn, &reg);
                id_ex.rs1 = rs1;
                id_ex.rs2 = rs2;
            }

        }

        if clock {
            // EX: Execution or address calculation
            let mut npc = id_ex.npc;

            let pc = if npc == 0 { 0 } else { npc - 4 };
            let mut insn = id_ex.insn;

            // ALU src1 mux
            let rs1: i32;
            if ex_hazard_src1(id_ex, ex_mem) {
                rs1 = ex_mem.alu_result; // forward from previous ALU result
            } else if mem_hazard_src1(id_ex, ex_mem, mem_wb) {
                rs1 = mem_wb.mem_result as i32; // forward from data memory
            } else {
                rs1 = id_ex.rs1;

            }

            // ALU src2 mux
            let rs2: i32;
            if ex_hazard_src2(id_ex, ex_mem) {
                rs2 = ex_mem.alu_result; // forward from previous ALU result
            } else if mem_hazard_src2(id_ex, ex_mem, mem_wb) {
                rs2 = mem_wb.mem_result as i32; // forward from data memory
            } else {
                rs2 = id_ex.rs2;

            }

            let alu_result = stages::execute(&mut insn, rs1, rs2);

            // Modify program counter for branch or jump
            if insn.semantics.branch &&
                !(insn.opcode == Opcode::Branch && alu_result != 0)
            {
                let imm = insn.fields.imm.unwrap() as i32;
                npc = match insn.opcode {
                    Opcode::Jalr => alu_result & 0xfffe, // LSB -> 0
                    _ => (pc as i32) + imm,
                } as u32;

            }

            if insn.function == Function::Halt {
                println!("Caught halt instruction at {:#0x}, exiting...", pc);
                return pc as u32;
            }

            ex_mem.npc = npc;
            ex_mem.insn = id_ex.insn;
            ex_mem.alu_result = alu_result;
            ex_mem.rs2 = rs2;
        } else {
            // MEM: Data memory access
            let insn = ex_mem.insn;
            let alu_result = ex_mem.alu_result;
            let rs2 = ex_mem.rs2;
            let mem_result =
                stages::access_memory(&insn, &mut mem, alu_result, rs2);

            mem_wb.insn = insn;
            mem_wb.alu_result = alu_result;
            mem_wb.mem_result = mem_result;
        }

        if clock {
            let insn = mem_wb.insn;
            let alu_result = mem_wb.alu_result;
            let mem_result = mem_wb.mem_result;

            // WB: Write result back to register
            stages::reg_writeback(&insn, &mut reg, alu_result, mem_result);

        }

        //println!("{:#0x} - {:?}", pc, insn);

        clock = !clock;
    }

}