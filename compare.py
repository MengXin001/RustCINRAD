import cProfile
from cinrad.io import CinradReader, StandardData
from cinrad.visualize import PPI
def r():
    tilt = 0
    drange = 460
    dtype = 'REF'
    f = CinradReader('Z_RADR_I_Z9515_20160623043100_O_DOR_SA_CAP.bin') #New standard data
    R = f.get_data(tilt, drange, dtype)
    fig = PPI(R)
    fig('radar_pycinrad.png')
if __name__ == '__main__':
    cProfile.run("r()")